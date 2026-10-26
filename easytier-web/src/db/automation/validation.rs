use std::collections::HashSet;

use serde_json::{Value, json};
use uuid::Uuid;

use super::{AutomationError, AutomationResult};

pub(super) fn keys(value:&Value, allowed:&[&str])->AutomationResult<()> {
    let Some(object)=value.as_object() else {return Err(AutomationError::field("body","An object is required."));};
    if object.keys().any(|key|!allowed.contains(&key.as_str())) {return Err(AutomationError::field("body","The request contains unsupported fields."));}
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
    keys(value,&["resource_id","description","text","class_name","package","xpath"])?;
    let object=value.as_object().ok_or_else(||AutomationError::field("selector","A selector is required."))?;
    if object.is_empty() {return Err(AutomationError::field("selector","Choose at least one selector field."));}
    if object.contains_key("xpath") && object.len()!=1 {
        return Err(AutomationError::field("selector","XPath cannot be combined with native selector fields."));
    }
    for field in object.keys() {
        if !value[field].as_str().is_some_and(|s|!s.trim().is_empty()&&s.len()<=512&&!s.contains('\0')) {
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
        let _=marked_step;
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
