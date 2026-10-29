use std::collections::HashSet;

use serde_json::{Value, json};
use uuid::Uuid;

use super::{AutomationError, AutomationResult};

pub(super) fn keys(value:&Value, allowed:&[&str])->AutomationResult<()> {
    let Some(object)=value.as_object() else {return Err(AutomationError::field("body","An object is required."));};
    if object.keys().any(|key|!allowed.contains(&key.as_str())
        && !(key=="title" && allowed.contains(&"id") && allowed.contains(&"type"))) {return Err(AutomationError::field("body","The request contains unsupported fields."));}
    Ok(())
}

pub(super) fn text(value:&Value,field:&str,max:usize,required:bool)->AutomationResult<String> {
    let result=match value.get(field) {
        None|Some(Value::Null) if !required=>String::new(),
        Some(Value::String(s))=>s.trim().to_owned(),
        _=>return Err(AutomationError::field(field,"A text value is required.")),
    };
    if (required&&result.is_empty())||result.len()>max||result.chars().any(|c|c=='\0') {return Err(AutomationError::field(field,"The value is empty, too long, or contains unsupported characters."));}
    Ok(result)
}

fn coordinate(step:&Value,field:&str)->AutomationResult<()> {
    if !step[field].as_f64().is_some_and(|n|n.is_finite()&&(0.0..=1.0).contains(&n)) {return Err(AutomationError::field(field,"Use a normalized coordinate from 0 to 1."));}
    Ok(())
}

fn dimension(step:&Value,field:&str)->AutomationResult<()> {
    if !step[field].as_u64().is_some_and(|n|(1..=16384).contains(&n)) {return Err(AutomationError::field(field,"The recorded screen dimensions are required."));}
    Ok(())
}

fn selector(value:&Value)->AutomationResult<()> {
    const BOOLEAN_FIELDS:&[&str]=&["clickable","enabled","long_clickable","focusable","checkable","scrollable"];
    keys(value,&["resource_id","description","text","class_name","package","xpath",
                 "clickable","enabled","long_clickable","focusable","checkable","scrollable"])?;
    let object=value.as_object().ok_or_else(||AutomationError::field("selector","A selector is required."))?;
    if object.is_empty() {return Err(AutomationError::field("selector","Choose at least one selector field."));}
    if object.contains_key("xpath") && object.len()!=1 {
        return Err(AutomationError::field("selector","XPath cannot be combined with native selector fields."));
    }
    for field in object.keys() {
        if BOOLEAN_FIELDS.contains(&field.as_str()) {
            if !value[field].is_boolean() {
                return Err(AutomationError::field(field,"Selector boolean must be true or false."));
            }
        } else if !value[field].as_str().is_some_and(|s|!s.trim().is_empty()&&s.len()<=512&&!s.contains('\0')) {
            return Err(AutomationError::field(field,"Selector text is empty or exceeds 512 bytes."));
        }
    }
    if object.contains_key("xpath") && !value["xpath"].as_str().is_some_and(|s|s.starts_with('/')||s.starts_with('(')) {
        return Err(AutomationError::field("xpath","XPath must begin with / or (."));
    }
    Ok(())
}

fn parameter_name(value:&str)->bool {
    !value.is_empty()&&value.len()<=64&&value.bytes().next().is_some_and(|c|c.is_ascii_alphabetic()||c==b'_')
        &&value.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_')
}

/// ADB steps are always pinned to the selected phone's existing connection.
pub(super) fn adb_shell_command(value:&Value)->AutomationResult<String> {
    let command=value.as_str().ok_or_else(||AutomationError::field("command","Enter an ADB shell command."))?;
    let trimmed=command.trim();
    let parts=trimmed.split_whitespace().collect::<Vec<_>>();
    if trimmed.len()>1024 || parts.len()<3 || parts[0]!="adb" || parts[1]!="shell"
        || trimmed.chars().any(|c|c.is_control() || ";|&<>use std::collections::HashSet;

use serde_json::{Value, json};
use uuid::Uuid;

use super::{AutomationError, AutomationResult};

pub(super) fn keys(value:&Value, allowed:&[&str])->AutomationResult<()> {
    let Some(object)=value.as_object() else {return Err(AutomationError::field("body","An object is required."));};
    if object.keys().any(|key|!allowed.contains(&key.as_str())
        && !(key=="title" && allowed.contains(&"id") && allowed.contains(&"type"))) {return Err(AutomationError::field("body","The request contains unsupported fields."));}
    Ok(())
}

pub(super) fn text(value:&Value,field:&str,max:usize,required:bool)->AutomationResult<String> {
    let result=match value.get(field) {
        None|Some(Value::Null) if !required=>String::new(),
        Some(Value::String(s))=>s.trim().to_owned(),
        _=>return Err(AutomationError::field(field,"A text value is required.")),
    };
    if (required&&result.is_empty())||result.len()>max||result.chars().any(|c|c=='\0') {return Err(AutomationError::field(field,"The value is empty, too long, or contains unsupported characters."));}
    Ok(result)
}

fn coordinate(step:&Value,field:&str)->AutomationResult<()> {
    if !step[field].as_f64().is_some_and(|n|n.is_finite()&&(0.0..=1.0).contains(&n)) {return Err(AutomationError::field(field,"Use a normalized coordinate from 0 to 1."));}
    Ok(())
}

fn dimension(step:&Value,field:&str)->AutomationResult<()> {
    if !step[field].as_u64().is_some_and(|n|(1..=16384).contains(&n)) {return Err(AutomationError::field(field,"The recorded screen dimensions are required."));}
    Ok(())
}

fn selector(value:&Value)->AutomationResult<()> {
    const BOOLEAN_FIELDS:&[&str]=&["clickable","enabled","long_clickable","focusable","checkable","scrollable"];
    keys(value,&["resource_id","description","text","class_name","package","xpath",
                 "clickable","enabled","long_clickable","focusable","checkable","scrollable"])?;
    let object=value.as_object().ok_or_else(||AutomationError::field("selector","A selector is required."))?;
    if object.is_empty() {return Err(AutomationError::field("selector","Choose at least one selector field."));}
    if object.contains_key("xpath") && object.len()!=1 {
        return Err(AutomationError::field("selector","XPath cannot be combined with native selector fields."));
    }
    for field in object.keys() {
        if BOOLEAN_FIELDS.contains(&field.as_str()) {
            if !value[field].is_boolean() {
                return Err(AutomationError::field(field,"Selector boolean must be true or false."));
            }
        } else if !value[field].as_str().is_some_and(|s|!s.trim().is_empty()&&s.len()<=512&&!s.contains('\0')) {
            return Err(AutomationError::field(field,"Selector text is empty or exceeds 512 bytes."));
        }
    }
    if object.contains_key("xpath") && !value["xpath"].as_str().is_some_and(|s|s.starts_with('/')||s.starts_with('(')) {
        return Err(AutomationError::field("xpath","XPath must begin with / or (."));
    }
    Ok(())
}

fn parameter_name(value:&str)->bool {
    !value.is_empty()&&value.len()<=64&&value.bytes().next().is_some_and(|c|c.is_ascii_alphabetic()||c==b'_')
        &&value.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_')
}

\\\r\n".contains(c))
        || matches!(parts[2],"sh"|"su"|"bash"|"ash"|"busybox"|"toybox") {
        return Err(AutomationError::field("command","Enter a single adb shell command (no host options, shell interpreters, pipes or redirects)."));
    }
    Ok(trimmed.to_owned())
}

/// Persisted scripts may be empty until their first step is recorded.
/// An executable run MUST still pass steps() (1–100 valid actions).
pub(super) fn saved_steps(input:&Value)->AutomationResult<Value> {
    if input.as_array().is_some_and(|items|items.is_empty()) {
        return Ok(json!([]));
    }
    steps(input)
}

pub(super) fn steps(input:&Value)->AutomationResult<Value> {
    let items=input.as_array().filter(|v|!v.is_empty()&&v.len()<=100).ok_or_else(||AutomationError::field("steps","Use between 1 and 100 steps."))?;
    if input.to_string().len()>256*1024 {return Err(AutomationError::field("steps","The script is too large."));}
    // Only newly marked scripts opt into business verification. Existing
    // assertions and historic runs retain their original unverified meaning.
    let marked=items.iter().filter(|step|step.get("business_verification").is_some()).collect::<Vec<_>>();
    if !marked.is_empty() {
        let last=items.last().unwrap();
        if marked.len()!=1 || !std::ptr::eq(marked[0],last)
            || !matches!(last["type"].as_str(),Some("assert"|"assert_property"))
            || last["business_verification"]!=true
        {
            return Err(AutomationError::field("business_verification",
                "Select only the final assert or assert_property step as the business check."));
        }
    }
    let mut ids=HashSet::new();
    for step in items {
        let id=step["id"].as_str().ok_or_else(||AutomationError::field("steps.id","Each step requires a distinct UUID."))?;
        if !Uuid::parse_str(id).is_ok_and(|uuid|uuid.to_string()==id)||!ids.insert(id) {return Err(AutomationError::field("steps.id","Each step requires a distinct canonical UUID."));}
        let kind=step["type"].as_str().ok_or_else(||AutomationError::field("steps.type","Choose a supported step."))?;
        if let Some(title)=step.get("title") {
            if !title.as_str().is_some_and(|s|!s.trim().is_empty() && s.chars().count()<=80
                && s.len()<=256 && !s.chars().any(char::is_control)) {
                return Err(AutomationError::field("title","Step title must be 1–80 visible characters."));
            }
        }
        match kind {
            "key"=>{
                keys(step,&["id","type","key"])?;
                if !matches!(step["key"].as_str(),Some("HOME"|"BACK"|"RECENT"|"WAKE")) {return Err(AutomationError::field("key","Choose a supported navigation key."));}
            },
            "tap"|"long_press"=>{
                keys(step,&["id","type","selector","x","y","width","height","duration_ms"])?;
                if let Some(value)=step.get("selector") {
                    selector(value)?;
                    if ["x","y","width","height"].iter().any(|f|step.get(*f).is_some()) {return Err(AutomationError::field("selector","Choose either a selector or screen coordinates."));}
                } else {coordinate(step,"x")?;coordinate(step,"y")?;dimension(step,"width")?;dimension(step,"height")?;}
                if kind=="tap" && step.get("duration_ms").is_some() {
                    return Err(AutomationError::field("duration_ms","Tap does not accept a duration."));
                }
                if kind=="long_press" && !step["duration_ms"].as_u64().is_some_and(|v|(400..=3000).contains(&v)) {
                    return Err(AutomationError::field("duration_ms","Long press duration must be 400 to 3000 ms."));
                }
            },
            "swipe"|"drag"=>{
                keys(step,&["id","type","x","y","to_x","to_y","duration_ms","width","height"])?;
                for f in ["x","y","to_x","to_y"] {coordinate(step,f)?;}
                dimension(step,"width")?;dimension(step,"height")?;
                let low=if kind=="drag" {300} else {100};
                if !step["duration_ms"].as_u64().is_some_and(|v|(low..=3000).contains(&v)) {return Err(AutomationError::field("duration_ms","Gesture duration is out of bounds."));}
            },
            "input_text"=>{
                keys(step,&["id","type","text","parameter","selector"])?;
                if step.get("text").is_some()==step.get("parameter").is_some() {return Err(AutomationError::field("text","Choose literal text or a runtime parameter."));}
                if let Some(value)=step.get("text") {
                    if !value.as_str().is_some_and(|s|s.len()<=2000&&!s.contains('\0')) {return Err(AutomationError::field("text","Text must be at most 2000 bytes."));}
                } else {
                    let name=step["parameter"].as_str().ok_or_else(||AutomationError::field("parameter","A parameter name is required."))?;
                    if !parameter_name(name){return Err(AutomationError::field("parameter","Use a variable name containing ASCII letters, digits or underscores."));}
                }
                if let Some(value)=step.get("selector") {
                    selector(value)?;
                    if value.get("xpath").is_some() {
                        return Err(AutomationError::field("selector","XPath input is not supported; choose native fields."));
                    }
                }
            },
            "launch_app"=>{
                keys(step,&["id","type","package"])?;
                let package=step["package"].as_str().ok_or_else(||AutomationError::field("package","Use an Android application package name."))?;
                if package.len()>255||!package.contains('.')||package.split('.').any(|part|part.is_empty()||!part.chars().all(|c|c.is_ascii_alphanumeric()||c=='_')||!part.chars().next().is_some_and(|c|c.is_ascii_alphabetic())) {return Err(AutomationError::field("package","Use an Android application package name."));}
            },
            "wait_for"|"assert"|"wait_gone"=>{
                if kind=="assert" { keys(step,&["id","type","selector","timeout_ms","business_verification"])?; }
                else { keys(step,&["id","type","selector","timeout_ms"])?; }
                selector(&step["selector"])?;
                if !step["timeout_ms"].as_u64().is_some_and(|v|(100..=30000).contains(&v)) {return Err(AutomationError::field("timeout_ms","Use a timeout from 100 to 30000 ms."));}
            },
            "if_gone"=>{
                keys(step,&["id","type","selector","skip_count"])?;
                selector(&step["selector"])?;
                if !step["skip_count"].as_u64().is_some_and(|v| (1..=20).contains(&v)) {
                    return Err(AutomationError::field("skip_count","Choose 1 to 20 steps."));
                }
                let index=ids.len()-1;
                if step["skip_count"].as_u64().unwrap_or(0) > (items.len()-index-1) as u64 {
                    return Err(AutomationError::field("skip_count","Skipped steps extend past the end of the script."));
                }
            },
            "if_else"=>{
                keys(step,&["id","type","selector","then_count","else_count"])?;
                selector(&step["selector"])?;
                let then_count=step["then_count"].as_u64().filter(|v|(1..=20).contains(v))
                    .ok_or_else(||AutomationError::field("then_count","Choose 1 to 20 then steps."))?;
                let else_count=step["else_count"].as_u64().filter(|v|(1..=20).contains(v))
                    .ok_or_else(||AutomationError::field("else_count","Choose 1 to 20 else steps."))?;
                let index=ids.len()-1;
                if then_count+else_count>(items.len()-index-1) as u64 {
                    return Err(AutomationError::field("then_count","The conditional blocks extend past the script end."));
                }
            },
            "assert_property"=>{
                keys(step,&["id","type","selector","property","expected","business_verification"])?;
                selector(&step["selector"])?;
                if step["selector"].get("xpath").is_some() {return Err(AutomationError::field("selector","Property checks currently require a native selector."));}
                if !matches!(step["property"].as_str(),Some("enabled"|"checked"|"selected"|"clickable"|"scrollable")) {
                    return Err(AutomationError::field("property","Unsupported boolean element property."));
                }
                if !step["expected"].is_boolean() {return Err(AutomationError::field("expected","Expected value must be a boolean."));}
            },
            "scroll_find"=>{
                keys(step,&["id","type","selector","scroll_container","direction","max_swipes","duration_ms","timeout_ms"])?;
                selector(&step["selector"])?;
                selector(&step["scroll_container"])?;
                if step["scroll_container"].get("xpath").is_some() {return Err(AutomationError::field("scroll_container","Scroll container requires native selector fields."));}
                if !matches!(step["direction"].as_str(),Some("up"|"down")) {return Err(AutomationError::field("direction","Choose up or down."));}
                if !step["max_swipes"].as_u64().is_some_and(|v|(1..=5).contains(&v)) {return Err(AutomationError::field("max_swipes","Use 1 to 5 swipes."));}
                if !step["duration_ms"].as_u64().is_some_and(|v|(200..=1000).contains(&v)) {return Err(AutomationError::field("duration_ms","Use 200 to 1000 ms."));}
                if !step["timeout_ms"].as_u64().is_some_and(|v|(100..=30000).contains(&v)) {return Err(AutomationError::field("timeout_ms","Use 100 to 30000 ms."));}
            },
            "screenshot"=>keys(step,&["id","type"])?,
            "adb_command"=>{
                keys(step,&["id","type","command","timeout_ms"])?;
                adb_shell_command(&step["command"])?;
                if !step["timeout_ms"].as_u64().is_some_and(|v|(1000..=20000).contains(&v)) {
                    return Err(AutomationError::field("timeout_ms","ADB command timeout must be 1000–20000 ms."));
                }
            },
            _=>return Err(AutomationError::field("steps.type","This action is not supported.")),
        }
    }
    // Conditions form a forward-only tree, never crossing a parent lane.
    // The wire protocol remains flat and compatible with older scripts.
    let mut regions:Vec<(usize,usize)>=Vec::new(); // (block end, Then end)
    for (index,step) in items.iter().enumerate() {
        while regions.last().is_some_and(|(end,_)|index>*end) {regions.pop();}
        let kind=step["type"].as_str().unwrap_or("");
        if !matches!(kind,"if_else"|"if_gone") {continue;}
        let then_count=if kind=="if_else" {step["then_count"].as_u64()} else {step["skip_count"].as_u64()}
            .ok_or_else(||AutomationError::field("steps","Invalid condition span."))? as usize;
        let else_count=if kind=="if_else" {step["else_count"].as_u64().unwrap_or(0) as usize} else {0};
        let then_end=index+then_count;
        let end=then_end+else_count;
        if let Some(&(parent_end,parent_then_end))=regions.last() {
            let ceiling=if index<=parent_then_end {parent_then_end} else {parent_end};
            if end>ceiling {return Err(AutomationError::field("steps","Nested condition crosses its parent Then/Else boundary."));}
        }
        if regions.len()>=3 {return Err(AutomationError::field("steps","Nested conditions may be at most three levels deep."));}
        regions.push((end,then_end));
    }
    // An assertion inside Then/Else may be skipped on one device and can
    // never prove the run-level business outcome. Reject such markers.
    if !marked.is_empty() {
        let index=items.len()-1;
        if items[..index].iter().enumerate().any(|(at,step)| {
            match step["type"].as_str() {
                Some("if_else") => at+step["then_count"].as_u64().unwrap_or(0) as usize
                    +step["else_count"].as_u64().unwrap_or(0) as usize >= index,
                Some("if_gone") => at+step["skip_count"].as_u64().unwrap_or(0) as usize >= index,
                _=>false,
            }
        }) {
            return Err(AutomationError::field("business_verification",
                "The business assertion must be outside all conditional lanes."));
        }
    }
    Ok(input.clone())
}

pub(super) fn parameters(value:&Value,steps:&Value)->AutomationResult<Value> {
    let empty=json!({});
    let value=if value.is_null(){&empty}else{value};
    let object=value.as_object().filter(|m|m.len()<=30).ok_or_else(||AutomationError::field("parameters","Use at most 30 named text parameters."))?;
    if object.iter().any(|(key,value)|!parameter_name(key)||!value.as_str().is_some_and(|s|s.len()<=2000&&!s.contains('\0'))) {return Err(AutomationError::field("parameters","Parameter names and text values are invalid or too long."));}
    for step in steps.as_array().into_iter().flatten() {
        if let Some(key)=step["parameter"].as_str() {
            if !object.contains_key(key) {return Err(AutomationError::field("parameters","A script parameter is missing."));}
        }
    }
    // Runtime input can be sensitive. Store only the required names, never values.
    Ok(json!(object.keys().collect::<Vec<_>>()))
}

#[cfg(test)]
mod nested_flow_tests {
    use super::*;

    fn key() -> Value {
        json!({"id": Uuid::new_v4().to_string(), "type":"key", "key":"HOME"})
    }

    fn if_gone(skip_count:usize) -> Value {
        json!({"id":Uuid::new_v4().to_string(),"type":"if_gone",
               "selector":{"text":"Loading"},"skip_count":skip_count})
    }

    fn if_else(then_count:usize,else_count:usize) -> Value {
        json!({"id":Uuid::new_v4().to_string(),"type":"if_else",
               "selector":{"text":"Loading"},"then_count":then_count,"else_count":else_count})
    }

    #[test]
    fn three_level_flow_is_accepted_and_fourth_level_rejected() {
        let valid=json!([
            if_else(5,1), if_gone(4), if_else(1,2),
            key(),key(),key(),key()
        ]);
        assert!(steps(&valid).is_ok());
        let invalid=json!([
            if_gone(5),if_gone(4),if_gone(3),if_gone(2),key(),key()
        ]);
        assert!(steps(&invalid).is_err());
    }

    #[test]
    fn child_condition_cannot_cross_then_else_boundary() {
        let invalid=json!([
            if_else(2,1),key(),if_gone(1),key()
        ]);
        assert!(steps(&invalid).is_err());
        let valid=json!([if_gone(3),if_else(1,1),key(),key()]);
        assert!(steps(&valid).is_ok());
    }
}


#[cfg(test)]
mod business_verification_contract_tests {
    use super::*;

    fn assert_step(marked: bool) -> Value {
        let mut step=json!({
            "id":Uuid::new_v4().to_string(),
            "type":"assert",
            "selector":{"text":"Operation successful"},
            "timeout_ms":10000
        });
        if marked {step["business_verification"]=json!(true);}
        step
    }

    #[test]
    fn legacy_assertion_does_not_get_marked_implicitly() {
        let legacy=json!([assert_step(false)]);
        let verified=json!([assert_step(true)]);
        assert!(steps(&legacy).is_ok());
        assert!(steps(&verified).is_ok());
        assert_eq!(steps(&legacy).unwrap()[0].get("business_verification"),None);
        assert_eq!(steps(&verified).unwrap()[0]["business_verification"],true);
    }

    #[test]
    fn verifier_must_be_one_final_assertion_and_explicitly_true() {
        let normal=json!({"id":Uuid::new_v4().to_string(),"type":"key","key":"BACK"});
        assert!(steps(&json!([assert_step(true),normal.clone()])).is_err());
        let mut illegal_key=normal.clone();
        illegal_key["business_verification"]=json!(true);
        assert!(steps(&json!([illegal_key])).is_err());
        let mut bad_marker=assert_step(false);
        bad_marker["business_verification"]=json!(false);
        assert!(steps(&json!([bad_marker])).is_err());
        assert!(steps(&json!([assert_step(true),assert_step(true)])).is_err());
        assert!(steps(&json!([normal,assert_step(true)])).is_ok());
    }

    #[test]
    fn final_business_check_cannot_be_inside_a_skippable_branch() {
        let if_gone=json!({
            "id":Uuid::new_v4().to_string(),"type":"if_gone",
            "selector":{"text":"Overlay"},"skip_count":1
        });
        assert!(steps(&json!([if_gone,assert_step(true)])).is_err());
    }

    #[test]
    fn property_assertion_can_verify_business_success() {
        let p=json!({
            "id":Uuid::new_v4().to_string(),
            "type":"assert_property","selector":{"resource_id":"com.example:id/ok"},
            "property":"enabled","expected":true,
            "business_verification":true
        });
        assert!(steps(&json!([p])).is_ok());
    }
}

#[cfg(test)]
mod empty_script_persistence_contract_tests {
    use super::*;

    #[test]
    fn empty_scripts_can_be_saved_but_are_not_executable() {
        let blank=json!([]);
        assert_eq!(saved_steps(&blank).unwrap(),blank);
        assert!(steps(&blank).is_err());
    }

    #[test]
    fn nonempty_saved_scripts_keep_existing_validation() {
        let valid=json!([{"id":Uuid::new_v4().to_string(),"type":"key","key":"WAKE"}]);
        assert_eq!(saved_steps(&valid).unwrap(),steps(&valid).unwrap());
        let bad=json!([{"id":Uuid::new_v4().to_string(),"type":"key","key":"INVALID"}]);
        assert!(saved_steps(&bad).is_err());
        assert!(saved_steps(&Value::Null).is_err());
        assert!(saved_steps(&json!({})).is_err());
        let many=(0..101).map(|_|json!({"id":Uuid::new_v4().to_string(),"type":"key","key":"HOME"})).collect::<Vec<_>>();
        assert!(saved_steps(&json!(many)).is_err());
    }
}

#[cfg(test)]
mod loc08_boolean_selector_regressions {
    use super::selector;
    use serde_json::json;

    #[test]
    fn accepts_true_and_false_without_coercing_or_breaking_old_selectors() {
        for key in ["clickable","enabled","long_clickable","focusable","checkable","scrollable"] {
            for value in [true,false] {
                let mut object=serde_json::Map::new();
                object.insert("description".into(),json!("领淘金币"));
                object.insert(key.into(),json!(value));
                assert!(selector(&serde_json::Value::Object(object)).is_ok(),"{key}={value}");
            }
        }
        assert!(selector(&json!({"resource_id":"demo:id/a"})).is_ok());
    }

    #[test]
    fn rejects_stringified_boolean_and_mixed_xpath() {
        for key in ["clickable","enabled","long_clickable","focusable","checkable","scrollable"] {
            for invalid in [json!("false"),json!(0),serde_json::Value::Null] {
                let mut object=serde_json::Map::new();
                object.insert(key.into(),invalid);
                assert!(selector(&serde_json::Value::Object(object)).is_err());
            }
        }
        assert!(selector(&json!({"xpath":"//*","enabled":true})).is_err());
    }
}
