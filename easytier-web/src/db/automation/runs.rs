use std::collections::{HashMap,HashSet};

use serde_json::{Value,json};
use sqlx::{Row,SqliteConnection};
use uuid::Uuid;

use super::{AutomationError,AutomationResult,AutomationStore,active_owner,authorized,conflict,digest,ensure_route_unambiguous,fingerprint,not_found,now,parse_json,target_key,validation};

fn terminal(state:&str)->bool {matches!(state,"completed"|"succeeded"|"failed"|"cancelled"|"interrupted")}

fn allowed_state(state:&str)->bool {
    matches!(state,"dispatch_pending"|"queued"|"accepted"|"starting"|"running"|"completed"|"succeeded"|"failed"|"cancel_requested"|"cancelled"|"interrupted"|"outcome_unknown"|"cleanup_pending")
}

fn aggregate(states:&[String])->&'static str {
    if states.iter().any(|s|matches!(s.as_str(),"queued"|"accepted"|"starting"|"running")) {return "running";}
    if states.iter().any(|s|s=="dispatch_pending") {return "dispatch_pending";}
    if states.iter().any(|s|s=="cancel_requested") {return "cancel_requested";}
    if states.iter().any(|s|matches!(s.as_str(),"outcome_unknown"|"cleanup_pending")) {return "outcome_unknown";}
    if states.iter().all(|s|matches!(s.as_str(),"completed"|"succeeded")) {return "completed";}
    if states.iter().all(|s|s=="cancelled") {return "cancelled";}
    if states.iter().all(|s|s=="interrupted") {return "interrupted";}
    if states.iter().all(|s|s=="failed") {return "failed";}
    "partial_failed"
}

async fn read_run(conn:&mut SqliteConnection,owner:i32,id:&str)->AutomationResult<Value> {
    let row=sqlx::query("SELECT * FROM automation_runs WHERE id=? AND owner_user_id=?").bind(id).bind(owner).fetch_optional(&mut *conn).await?.ok_or_else(not_found)?;
    let rows=sqlx::query("SELECT * FROM automation_run_devices WHERE run_id=? ORDER BY ordinal").bind(id).fetch_all(&mut *conn).await?;
    let mut devices=Vec::with_capacity(rows.len());
    for d in rows {
        let current:Option<String>=d.try_get("current_step")?;
        let error:Option<String>=d.try_get("error_json")?;
        devices.push(json!({"device_id":d.try_get::<String,_>("device_id")?,"name":d.try_get::<String,_>("name")?,
            "withdraw_requested":d.try_get::<bool,_>("withdraw_requested")?,"cleanup_confirmed":d.try_get::<bool,_>("cleanup_confirmed")?,
            "state":d.try_get::<String,_>("state")?,"current_step":current.map(|s|serde_json::from_str::<Value>(&s).unwrap_or(Value::String(s))),
            "error":error.map(|s|parse_json(&s)).transpose()?,"steps":parse_json(&d.try_get::<String,_>("steps_json")?)?,
            "updated_at":d.try_get::<String,_>("updated_at")?}));
    }
    Ok(json!({"id":id,"name":row.try_get::<String,_>("name")?,"state":row.try_get::<String,_>("state")?,
        "executor_id":row.try_get::<Option<String>,_>("runner_identity")?,
        "executor_revision":row.try_get::<i64,_>("result_revision")?,
        "created_at":row.try_get::<String,_>("created_at")?,"updated_at":row.try_get::<String,_>("updated_at")?,
        "script_snapshot":parse_json(&row.try_get::<String,_>("snapshot_json")?)?,
        "parameter_names":parse_json(&row.try_get::<String,_>("parameters_json")?)?,
        "concurrency":row.try_get::<i64,_>("concurrency")?,"cancel_requested":row.try_get::<bool,_>("cancel_requested")?,"devices":devices}))
}

fn safe_error(input:&Value)->AutomationResult<Value> {
    if input.is_null(){return Ok(Value::Null);}
    if let Some(s)=input.as_str() {
        if s.len()>2000 {return Err(AutomationError::field("error","The error response is too large."));}
        return Ok(json!(s));
    }
    validation::keys(input,&["code","message"])?;
    Ok(json!({"code":validation::text(input,"code",128,false)?,"message":validation::text(input,"message",2000,false)?}))
}

fn step_state_rank(state:&str)->u8 {
    match state {"pending"|"queued"|"not_started"=>0,"accepted"|"running"|"started"=>1,"action_sent"=>2,_=>3}
}

fn sanitized_steps(incoming:&Value,script:&Value,previous:&Value)->AutomationResult<Value> {
    let definitions=script["steps"].as_array().ok_or_else(not_found)?;
    let order:HashMap<&str,usize>=definitions.iter().enumerate().filter_map(|(i,s)|s["id"].as_str().map(|id|(id,i))).collect();
    let entries=incoming.as_array().filter(|v|v.len()<=100).ok_or_else(||AutomationError::field("steps","Invalid step results."))?;
    let mut merged:HashMap<String,Value>=previous.as_array().into_iter().flatten().filter_map(|s|s["step_id"].as_str().map(|id|(id.to_owned(),s.clone()))).collect();
    let mut seen=HashSet::new();
    for item in entries {
        let id=validation::text(item,"step_id",64,true)?;
        if !order.contains_key(id.as_str())||!seen.insert(id.clone()) {return Err(AutomationError::field("step_id","Result does not match the fixed script steps."));}
        let state=validation::text(item,"state",64,true)?;
        if !matches!(state.as_str(),"pending"|"queued"|"not_started"|"accepted"|"running"|"started"|"action_sent"|"action_completed"|"assertion_passed"|"completed"|"succeeded"|"passed"|"failed"|"cancelled"|"interrupted"|"outcome_unknown"|"skipped") {return Err(AutomationError::field("state","Invalid step result state."));}
        let artifact=match item.get("artifact_id").filter(|v|!v.is_null()) {
            Some(value)=>{
                let identifier=value.as_str().filter(|s|Uuid::parse_str(s).is_ok()).ok_or_else(||AutomationError::field("artifact_id","Use a valid screenshot identifier."))?;
                let mime=item["mime_type"].as_str().filter(|s|matches!(*s,"image/png"|"image/jpeg")).ok_or_else(||AutomationError::field("mime_type","Unsupported screenshot format."))?;
                let size=item["size"].as_u64().filter(|v|(1..=8*1024*1024).contains(v)).ok_or_else(||AutomationError::field("size","Screenshot size is invalid or exceeds 8 MiB."))?;
                Some(json!({"artifact_id":identifier,"mime_type":mime,"size":size}))
            },
            None=>None,
        };
        if let Some(old)=merged.get_mut(&id) {
            let old_state=old["state"].as_str().unwrap_or("pending");
            // A delayed progress report must not replace an already final result.
            if step_state_rank(old_state)>step_state_rank(&state)||(step_state_rank(old_state)==3&&old_state!="outcome_unknown") {
                if old_state==state && old["artifact_id"].is_null() {
                    if let Some(artifact)=artifact {for field in ["artifact_id","mime_type","size"] {old[field]=artifact[field].clone();}}
                }
                continue;
            }
        }
        let mut sanitized=json!({"step_id":id,"state":state});
        for field in ["message","started_at","finished_at"] {
            if !item[field].is_null() {sanitized[field]=json!(validation::text(item,field,if field=="message"{2000}else{256},false)?);}
        }
        if let Some(artifact)=artifact {for field in ["artifact_id","mime_type","size"] {sanitized[field]=artifact[field].clone();}}
        merged.insert(id,sanitized);
    }
    let mut values:Vec<Value>=merged.into_values().collect();
    values.sort_by_key(|s|order.get(s["step_id"].as_str().unwrap_or("")).copied().unwrap_or(usize::MAX));
    Ok(json!(values))
}

impl AutomationStore {
    pub(crate) async fn create_run(&self,actor:i32,owner:i32,body:&Value)->AutomationResult<(Value,bool)> {
        validation::keys(body,&["request_id","script_id","revision","name","steps","device_ids","parameters","concurrency"])?;
        self.authorize(actor,owner).await?;
        let request_id=validation::text(body,"request_id",64,true)?;
        if Uuid::parse_str(&request_id).is_err(){return Err(AutomationError::field("request_id","Use a unique UUID for this request."));}
        let request_hash=digest(body);
        if let Some(row)=sqlx::query("SELECT id,request_hash FROM automation_runs WHERE actor_user_id=? AND owner_user_id=? AND request_id=?")
            .bind(actor).bind(owner).bind(&request_id).fetch_optional(&self.pool).await? {
            if row.try_get::<String,_>("request_hash")?!=request_hash{return Err(conflict("idempotency_conflict","This request identifier was already used with different input."));}
            return Ok((self.run(owner,&row.try_get::<String,_>("id")?).await?,false));
        }
        let ids=body["device_ids"].as_array().filter(|v|!v.is_empty()&&v.len()<=20).ok_or_else(||AutomationError::field("device_ids","Select between 1 and 20 devices."))?;
        let mut seen=HashSet::new();let mut targets=Vec::with_capacity(ids.len());let mut endpoints=HashSet::new();
        for value in ids {
            let id=value.as_str().filter(|id|Uuid::parse_str(id).is_ok()).ok_or_else(||AutomationError::field("device_ids","Use valid device identifiers."))?;
            if !seen.insert(id.to_owned()){return Err(AutomationError::field("device_ids","A device can appear only once."));}
            let target=self.target(owner,id).await?;
            if !endpoints.insert(target_key(&target)?){return Err(conflict("route_ambiguous","Two selected assets resolve to the same ADB endpoint. Review their network mappings."));}
            targets.push(target);
        }
        let concurrency=match body.get("concurrency") {
            None=>1,
            Some(value)=>value.as_i64().ok_or_else(||AutomationError::field("concurrency","Choose an integer from one to three."))?,
        };
        if !(1..=3).contains(&concurrency){return Err(AutomationError::field("concurrency","Choose one to three simultaneous devices."));}
        if body.get("script_id").is_some_and(|value|!value.is_string()) {return Err(AutomationError::field("script_id","Use a saved script identifier or omit it for inline steps."));}
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        authorized(&mut tx,actor,owner).await?;
        // Close the check/insert race for simultaneous retries.
        if let Some(row)=sqlx::query("SELECT id,request_hash FROM automation_runs WHERE actor_user_id=? AND owner_user_id=? AND request_id=?")
            .bind(actor).bind(owner).bind(&request_id).fetch_optional(&mut *tx).await? {
            if row.try_get::<String,_>("request_hash")?!=request_hash{return Err(conflict("idempotency_conflict","This request identifier was already used with different input."));}
            let result=read_run(&mut tx,owner,&row.try_get::<String,_>("id")?).await?;
            tx.commit().await?;return Ok((result,false));
        }
        let snapshot=if let Some(script_id)=body["script_id"].as_str() {
            if body.get("steps").is_some(){return Err(AutomationError::field("steps","Choose a saved draft or inline steps, not both."));}
            let revision=body["revision"].as_i64().filter(|n|*n>0).ok_or_else(||AutomationError::field("revision","The selected draft revision is required."))?;
            let row=sqlx::query("SELECT name,revision,steps_json FROM automation_scripts WHERE id=? AND owner_user_id=?").bind(script_id).bind(owner).fetch_optional(&mut *tx).await?.ok_or_else(not_found)?;
            if revision!=row.try_get::<i64,_>("revision")? {return Err(conflict("script_revision_changed","The script changed after it was selected. Review it before running."));}
            json!({"schema_version":1,"script_id":script_id,"revision":revision,"name":row.try_get::<String,_>("name")?,"steps":validation::steps(&parse_json(&row.try_get::<String,_>("steps_json")?)?)?})
        } else {
            if body.get("revision").is_some(){return Err(AutomationError::field("revision","Inline steps do not have a saved draft revision."));}
            let name=if body.get("name").is_some(){validation::text(body,"name",128,true)?}else{"Script trial".into()};
            json!({"schema_version":1,"name":name,"steps":validation::steps(&body["steps"])?})
        };
        let parameter_names=validation::parameters(&body["parameters"],&snapshot["steps"])?;
        for target in &targets {
            let id=target["device_id"].as_str().ok_or_else(not_found)?;
            if fingerprint(&mut tx,owner,id,&self.runtime_observations).await?!=target["target_fingerprint"].as_str().unwrap_or(""){return Err(conflict("target_changed","A device changed before the task was saved."));}
            ensure_route_unambiguous(&mut tx,owner,id,target["ip"].as_str().ok_or_else(not_found)?,target["adb_port"].as_i64().ok_or_else(not_found)?).await?;
            let occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_run_devices WHERE (device_id=? OR target_key=?) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending'))")
                .bind(id).bind(target_key(target)?).fetch_one(&mut *tx).await?;
            if occupied{return Err(conflict("device_busy","A selected device has an unfinished task or unconfirmed result. Resolve that task before starting another."));}
        }
        let id=Uuid::new_v4().to_string();let timestamp=now();
        sqlx::query("INSERT INTO automation_runs(id,actor_user_id,owner_user_id,request_id,request_hash,name,state,snapshot_json,parameters_json,concurrency,created_at,updated_at) VALUES(?,?,?,?,?,?,'dispatch_pending',?,?,?,?,?)")
            .bind(&id).bind(actor).bind(owner).bind(request_id).bind(request_hash).bind(snapshot["name"].as_str()).bind(snapshot.to_string()).bind(parameter_names.to_string()).bind(concurrency).bind(&timestamp).bind(&timestamp).execute(&mut *tx).await?;
        for (ordinal,target) in targets.iter().enumerate() {
            sqlx::query("INSERT INTO automation_run_devices(run_id,device_id,ordinal,name,target_key,target_fingerprint,target_json,state,steps_json,updated_at) VALUES(?,?,?,?,?,?,?,'dispatch_pending','[]',?)")
                .bind(&id).bind(target["device_id"].as_str()).bind(ordinal as i64).bind(target["name"].as_str()).bind(target_key(target)?).bind(target["target_fingerprint"].as_str()).bind(target.to_string()).bind(&timestamp).execute(&mut *tx).await?;
        }
        let result=read_run(&mut tx,owner,&id).await?;tx.commit().await?;Ok((result,true))
    }

    pub(crate) async fn list_runs(&self,owner:i32)->AutomationResult<Value> {
        let mut conn=self.pool.acquire().await?;active_owner(&mut conn,owner).await?;
        let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM automation_runs WHERE owner_user_id=? ORDER BY created_at DESC,id DESC LIMIT 100").bind(owner).fetch_all(&mut *conn).await?;
        let mut items=Vec::with_capacity(ids.len());for id in ids{items.push(read_run(&mut conn,owner,&id).await?);}
        Ok(json!({"items":items}))
    }

    pub(crate) async fn run(&self,owner:i32,id:&str)->AutomationResult<Value> {
        let mut conn=self.pool.acquire().await?;read_run(&mut conn,owner,id).await
    }

    pub(crate) async fn bind_run_executor(&self,owner:i32,id:&str,executor_id:&str)->AutomationResult<()> {
        if Uuid::parse_str(executor_id).is_err(){return Err(AutomationError::field("executor_id","Use a valid executor identity."));}
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row=sqlx::query("SELECT actor_user_id,cancel_requested,state,runner_identity FROM automation_runs WHERE id=? AND owner_user_id=?")
            .bind(id).bind(owner).fetch_optional(&mut *tx).await?.ok_or_else(not_found)?;
        authorized(&mut tx,row.try_get("actor_user_id")?,owner).await?;
        if row.try_get::<bool,_>("cancel_requested")?||row.try_get::<String,_>("state")?!="dispatch_pending" {
            return Err(conflict("run_cancelled","This task can no longer be dispatched."));
        }
        let existing:Option<String>=row.try_get("runner_identity")?;
        if existing.as_deref().is_some_and(|value|value!=executor_id){return Err(conflict("executor_changed","This task is bound to a different executor identity."));}
        sqlx::query("UPDATE automation_runs SET runner_identity=?,updated_at=? WHERE id=?")
            .bind(executor_id).bind(now()).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;Ok(())
    }

    // The caller has verified an authenticated durable rejection fence from the
    // very same executor. A missing HTTP result alone is never this proof.
    pub(crate) async fn confirm_run_never_started(&self,owner:i32,id:&str,executor_id:&str)->AutomationResult<Value> {
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row=sqlx::query("SELECT cancel_requested,runner_identity FROM automation_runs WHERE id=? AND owner_user_id=?")
            .bind(id).bind(owner).fetch_optional(&mut *tx).await?.ok_or_else(not_found)?;
        if !row.try_get::<bool,_>("cancel_requested")?||row.try_get::<Option<String>,_>("runner_identity")?.as_deref()!=Some(executor_id){
            return Err(conflict("executor_mismatch","The cancellation proof does not match this task's executor."));
        }
        let timestamp=now();
        sqlx::query("UPDATE automation_run_devices SET state='cancelled',cleanup_confirmed=1,updated_at=? WHERE run_id=? AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending')")
            .bind(&timestamp).bind(id).execute(&mut *tx).await?;
        let states:Vec<String>=sqlx::query_scalar("SELECT state FROM automation_run_devices WHERE run_id=?").bind(id).fetch_all(&mut *tx).await?;
        sqlx::query("UPDATE automation_runs SET state=?,never_started_confirmed_at=?,updated_at=? WHERE id=?")
            .bind(aggregate(&states)).bind(&timestamp).bind(&timestamp).bind(id).execute(&mut *tx).await?;
        let result=read_run(&mut tx,owner,id).await?;tx.commit().await?;Ok(result)
    }

    pub(crate) async fn dispatch_payload(&self,owner:i32,id:&str)->AutomationResult<Value> {
        let mut conn=self.pool.acquire().await?;
        let row=sqlx::query("SELECT * FROM automation_runs WHERE id=? AND owner_user_id=?").bind(id).bind(owner).fetch_optional(&mut *conn).await?.ok_or_else(not_found)?;
        authorized(&mut conn,row.try_get("actor_user_id")?,owner).await?;
        if row.try_get::<bool,_>("cancel_requested")? {return Err(conflict("run_cancelled","This task is no longer authorized."));}
        let snapshot=parse_json(&row.try_get::<String,_>("snapshot_json")?)?;
        let target_rows:Vec<String>=sqlx::query_scalar("SELECT target_json FROM automation_run_devices WHERE run_id=? ORDER BY ordinal").bind(id).fetch_all(&mut *conn).await?;
        let targets=target_rows.iter().map(|raw|parse_json(raw)).collect::<AutomationResult<Vec<_>>>()?;
        Ok(json!({"id":id,"owner_user_id":owner,"actor_user_id":row.try_get::<i32,_>("actor_user_id")?,"name":row.try_get::<String,_>("name")?,
            "steps":snapshot["steps"],"parameters":{},"concurrency":row.try_get::<i64,_>("concurrency")?,"targets":targets}))
    }

    pub(crate) async fn set_run_dispatch_unknown(&self,owner:i32,id:&str)->AutomationResult<()> {
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_runs WHERE id=? AND owner_user_id=?)").bind(id).bind(owner).fetch_one(&mut *tx).await?;
        if !exists {return Err(not_found());}
        sqlx::query("UPDATE automation_run_devices SET state='outcome_unknown',updated_at=? WHERE run_id=? AND state IN ('dispatch_pending','queued','accepted','starting')").bind(now()).bind(id).execute(&mut *tx).await?;
        let states:Vec<String>=sqlx::query_scalar("SELECT state FROM automation_run_devices WHERE run_id=?").bind(id).fetch_all(&mut *tx).await?;
        sqlx::query("UPDATE automation_runs SET state=?,updated_at=? WHERE id=?").bind(aggregate(&states)).bind(now()).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;Ok(())
    }

    pub(crate) async fn cancel_run(&self,owner:i32,id:&str)->AutomationResult<Value> {
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let original=read_run(&mut tx,owner,id).await?;
        if !matches!(original["state"].as_str(),Some("completed"|"succeeded"|"failed"|"cancelled"|"interrupted"|"partial_failed")) {
            // Binding is persisted before POST. NULL is durable proof that this
            // task was never sent, and this transaction fences any late binding.
            let next=if original["executor_id"].is_null(){"cancelled"}else{"cancel_requested"};
            sqlx::query("UPDATE automation_run_devices SET state=?,cleanup_confirmed=CASE WHEN ?='cancelled' THEN 1 ELSE cleanup_confirmed END,updated_at=? WHERE run_id=? AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending')").bind(next).bind(next).bind(now()).bind(id).execute(&mut *tx).await?;
            let states:Vec<String>=sqlx::query_scalar("SELECT state FROM automation_run_devices WHERE run_id=?").bind(id).fetch_all(&mut *tx).await?;
            sqlx::query("UPDATE automation_runs SET cancel_requested=1,state=?,updated_at=? WHERE id=?").bind(aggregate(&states)).bind(now()).bind(id).execute(&mut *tx).await?;
        }
        let result=read_run(&mut tx,owner,id).await?;tx.commit().await?;Ok(result)
    }

    pub(crate) async fn authorize_run_step(&self,id:&str,device_id:&str,expected:&str,executor_id:&str)->AutomationResult<bool> {
        let row=sqlx::query("SELECT r.owner_user_id,r.actor_user_id,r.cancel_requested,r.runner_identity,d.target_fingerprint,d.state,d.withdraw_requested FROM automation_runs r JOIN automation_run_devices d ON d.run_id=r.id WHERE r.id=? AND d.device_id=?")
            .bind(id).bind(device_id).fetch_optional(&self.pool).await?;
        let Some(row)=row else{return Ok(false);};
        if row.try_get::<bool,_>("withdraw_requested")? || row.try_get::<Option<String>,_>("runner_identity")?.as_deref()!=Some(executor_id)||row.try_get::<bool,_>("cancel_requested")?||row.try_get::<String,_>("target_fingerprint")?!=expected||!matches!(row.try_get::<String,_>("state")?.as_str(),"dispatch_pending"|"queued"|"accepted"|"starting"|"running") {return Ok(false);}
        let owner=row.try_get("owner_user_id")?;let actor=row.try_get("actor_user_id")?;
        match self.authorize(actor,owner).await {Ok(())=>{},Err(e) if e.status<500=>return Ok(false),Err(e)=>return Err(e)}
        let current=match self.target(owner,device_id).await {Ok(target)=>target,Err(e) if e.status<500=>return Ok(false),Err(e)=>return Err(e)};
        if current["target_fingerprint"].as_str()!=Some(expected){return Ok(false);}
        // The prior checks involve separate read requests. Recheck persisted revocation
        // and the routing source in one short transaction immediately before granting.
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        match authorized(&mut tx,actor,owner).await {Ok(())=>{},Err(e) if e.status<500=>return Ok(false),Err(e)=>return Err(e)}
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_runs r JOIN automation_run_devices d ON d.run_id=r.id WHERE r.id=? AND d.device_id=? AND r.cancel_requested=0 AND d.withdraw_requested=0 AND r.runner_identity=? AND d.state IN ('dispatch_pending','queued','accepted','starting','running') AND d.target_fingerprint=?)")
            .bind(id).bind(device_id).bind(executor_id).bind(expected).fetch_one(&mut *tx).await?;
        let valid=valid&&fingerprint(&mut tx,owner,device_id,&self.runtime_observations).await?==expected;
        tx.commit().await?;Ok(valid)
    }

    pub(crate) async fn apply_run_result(&self,event_id:&str,id:&str,result:&Value)->AutomationResult<Value> {
        if event_id.is_empty()||event_id.len()>128||result.to_string().len()>1024*1024 {return Err(AutomationError::field("event_id","The result event is invalid or too large."));}
        if result["id"].as_str()!=Some(id){return Err(conflict("run_result_mismatch","Result identifier does not match the task."));}
        let revision=result["revision"].as_i64().filter(|value|*value>=0).ok_or_else(||AutomationError::field("revision","A monotonic executor result revision is required."))?;
        let hash=digest(result);let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let run=sqlx::query("SELECT owner_user_id,snapshot_json,cancel_requested,runner_identity,never_started_confirmed_at,result_revision,result_hash FROM automation_runs WHERE id=?").bind(id).fetch_optional(&mut *tx).await?.ok_or_else(not_found)?;
        let owner:i32=run.try_get("owner_user_id")?;
        let executor:Option<String>=run.try_get("runner_identity")?;
        if executor.is_none()||result["executor_id"].as_str()!=executor.as_deref(){return Err(conflict("executor_mismatch","The result did not come from this task's executor."));}
        if run.try_get::<Option<String>,_>("never_started_confirmed_at")?.is_some(){return Err(conflict("run_cancelled","The executor confirmed this task never started; later results are rejected."));}
        if let Some(event)=sqlx::query("SELECT run_id,payload_hash FROM automation_result_events WHERE event_id=?").bind(event_id).fetch_optional(&mut *tx).await? {
            if event.try_get::<String,_>("run_id")?!=id||event.try_get::<String,_>("payload_hash")?!=hash {return Err(conflict("event_conflict","This event identifier was reused with different data."));}
            let value=read_run(&mut tx,owner,id).await?;tx.commit().await?;return Ok(value);
        }
        let incoming_devices=result["devices"].as_array().filter(|items|items.len()<=20)
            .ok_or_else(||AutomationError::field("devices","Invalid result device list."))?;
        let expected_ids:Vec<String>=sqlx::query_scalar("SELECT device_id FROM automation_run_devices WHERE run_id=?")
            .bind(id).fetch_all(&mut *tx).await?;
        let incoming_ids:HashSet<&str>=incoming_devices.iter().filter_map(|device|device["device_id"].as_str()).collect();
        if incoming_devices.len()!=expected_ids.len()||incoming_ids.len()!=expected_ids.len()||expected_ids.iter().any(|expected|!incoming_ids.contains(expected.as_str())) {
            return Err(conflict("run_result_mismatch","The result must contain exactly the fixed task device set."));
        }
        let applied:i64=run.try_get("result_revision")?;
        if revision<=applied {
            if revision==applied&&run.try_get::<Option<String>,_>("result_hash")?.as_deref()!=Some(hash.as_str()) {
                return Err(conflict("result_revision_conflict","The executor reused a result revision with different contents."));
            }
            // A full result for another device can arrive late. Persist its event
            // receipt but never rewind progress or reoccupy a cleaned target.
            sqlx::query("INSERT INTO automation_result_events(event_id,run_id,payload_hash,created_at) VALUES(?,?,?,?)")
                .bind(event_id).bind(id).bind(&hash).bind(now()).execute(&mut *tx).await?;
            let value=read_run(&mut tx,owner,id).await?;tx.commit().await?;return Ok(value);
        }
        let snapshot=parse_json(&run.try_get::<String,_>("snapshot_json")?)?;
        let cancelling:bool=run.try_get("cancel_requested")?;
        let definitions=snapshot["steps"].as_array().ok_or_else(not_found)?;
        let devices=result["devices"].as_array().filter(|v|v.len()<=20).ok_or_else(||AutomationError::field("devices","Invalid result device list."))?;
        let expected_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM automation_run_devices WHERE run_id=?").bind(id).fetch_one(&mut *tx).await?;
        if devices.len()!=expected_count as usize{return Err(conflict("run_result_mismatch","The result must contain the fixed task device set."));}
        let mut seen=HashSet::new();
        for device in devices {
            let device_id=validation::text(device,"device_id",64,true)?;
            if !seen.insert(device_id.clone()){return Err(conflict("run_result_mismatch","The result contains duplicate devices."));}
            let old=sqlx::query("SELECT state,steps_json,current_step,cleanup_confirmed,withdraw_requested FROM automation_run_devices WHERE run_id=? AND device_id=?").bind(id).bind(&device_id).fetch_optional(&mut *tx).await?.ok_or_else(||conflict("run_result_mismatch","The result contains a device outside this task."))?;
            let old_state:String=old.try_get("state")?;
            let cancelling = cancelling || old.try_get::<bool,_>("withdraw_requested")?;
            let incoming=validation::text(device,"state",64,true)?;
            if !allowed_state(&incoming){return Err(AutomationError::field("state","Invalid device result state."));}
            let cleanup=match device.get("cleanup_confirmed") {
                None=>false,
                Some(value)=>value.as_bool().ok_or_else(||AutomationError::field("cleanup_confirmed","Cleanup confirmation must be a boolean."))?,
            };
            if cleanup&&(!cancelling||(!terminal(&incoming)&&incoming!="outcome_unknown")) {
                return Err(conflict("cleanup_unconfirmed","Only an explicitly cancelled task may confirm stopped execution while preserving its action outcome."));
            }
            let was_cleaned:bool=old.try_get("cleanup_confirmed")?;
            let mut next=incoming.as_str();
            if terminal(&old_state)||was_cleaned{
                let previous=parse_json(&old.try_get::<String,_>("steps_json")?)?;
                let steps=sanitized_steps(device.get("steps").unwrap_or(&json!([])),&snapshot,&previous)?;
                sqlx::query("UPDATE automation_run_devices SET steps_json=?,cleanup_confirmed=MAX(cleanup_confirmed,?) WHERE run_id=? AND device_id=?")
                    .bind(steps.to_string()).bind(cleanup).bind(id).bind(&device_id).execute(&mut *tx).await?;
                continue;
            }
            if cancelling&&!terminal(next)&&next!="outcome_unknown"&&next!="cleanup_pending" {next="cancel_requested";}
            if old_state=="running"&&matches!(next,"dispatch_pending"|"queued"|"accepted"|"starting"){next="running";}
            let current=match device.get("current_step").filter(|v|!v.is_null()) {
                Some(Value::String(step)) if definitions.iter().any(|s|s["id"].as_str()==Some(step.as_str()))=>Some(json!(step)),
                Some(value) if value.as_u64().is_some_and(|n|n<=definitions.len() as u64)=>Some(value.clone()),
                Some(_)=>return Err(AutomationError::field("current_step","The result references an invalid step.")),
                None=>None,
            };
            let previous=parse_json(&old.try_get::<String,_>("steps_json")?)?;
            let steps=sanitized_steps(device.get("steps").unwrap_or(&json!([])),&snapshot,&previous)?;
            let error=safe_error(&device["error"])?;
            sqlx::query("UPDATE automation_run_devices SET state=?,cleanup_confirmed=?,current_step=COALESCE(?,current_step),error_json=?,steps_json=?,updated_at=? WHERE run_id=? AND device_id=?")
                .bind(next).bind(cleanup).bind(current.map(|v|v.to_string())).bind(if error.is_null(){None}else{Some(error.to_string())}).bind(steps.to_string()).bind(now()).bind(id).bind(&device_id).execute(&mut *tx).await?;
        }
        let states:Vec<String>=sqlx::query_scalar("SELECT state FROM automation_run_devices WHERE run_id=?").bind(id).fetch_all(&mut *tx).await?;
        sqlx::query("UPDATE automation_runs SET state=?,result_revision=?,result_hash=?,updated_at=? WHERE id=?").bind(aggregate(&states)).bind(revision).bind(&hash).bind(now()).bind(id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO automation_result_events(event_id,run_id,payload_hash,created_at) VALUES(?,?,?,?)").bind(event_id).bind(id).bind(hash).bind(now()).execute(&mut *tx).await?;
        let output=read_run(&mut tx,owner,id).await?;tx.commit().await?;Ok(output)
    }
}

impl AutomationStore {
    /// Fence only this device. The immutable target snapshot and all other devices remain intact.
    pub(crate) async fn withdraw_device_runs(&self, owner: i32, device: &str, event: &str) -> AutomationResult<Vec<Value>> {
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let rows=sqlx::query("SELECT d.run_id,d.target_fingerprint,r.runner_identity FROM automation_run_devices d JOIN automation_runs r ON r.id=d.run_id WHERE r.owner_user_id=? AND d.device_id=? AND d.cleanup_confirmed=0 AND d.state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending')")
            .bind(owner).bind(device).fetch_all(&mut *tx).await?;
        let mut pending=Vec::new();
        for row in rows {
            let run:String=row.try_get("run_id")?; let executor:Option<String>=row.try_get("runner_identity")?;
            let state=if executor.is_none(){"cancelled"}else{"cancel_requested"};
            sqlx::query("UPDATE automation_run_devices SET withdraw_requested=1,withdraw_event_id=?,state=?,cleanup_confirmed=?,updated_at=? WHERE run_id=? AND device_id=?")
                .bind(event).bind(state).bind(executor.is_none()).bind(now()).bind(&run).bind(device).execute(&mut *tx).await?;
            let states:Vec<String>=sqlx::query_scalar("SELECT state FROM automation_run_devices WHERE run_id=?").bind(&run).fetch_all(&mut *tx).await?;
            sqlx::query("UPDATE automation_runs SET state=?,updated_at=? WHERE id=?").bind(aggregate(&states)).bind(now()).bind(&run).execute(&mut *tx).await?;
            pending.push(json!({"run_id":run,"device_id":device,"executor_id":executor,"old_target_fingerprint":row.try_get::<String,_>("target_fingerprint")?,"cleanup_confirmed":executor.is_none()}));
        }
        tx.commit().await?; Ok(pending)
    }
    pub(crate) async fn confirm_device_never_started(&self,owner:i32,run:&str,device:&str,executor:&str,fingerprint:&str)->AutomationResult<()> {
        let changed=sqlx::query("UPDATE automation_run_devices SET state='cancelled',cleanup_confirmed=1,updated_at=? WHERE run_id=? AND device_id=? AND withdraw_requested=1 AND target_fingerprint=? AND EXISTS(SELECT 1 FROM automation_runs WHERE id=? AND owner_user_id=? AND runner_identity=?)")
            .bind(now()).bind(run).bind(device).bind(fingerprint).bind(run).bind(owner).bind(executor).execute(&self.pool).await?;
        if changed.rows_affected()!=1 {return Err(conflict("withdraw_mismatch","The device exit proof does not match the original target."));}
        Ok(())
    }
}
