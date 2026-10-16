//! Claude Code's native formatted reply is the conversation output boundary.
use serde_json::Value;
#[derive(Default)]
pub struct ReplyStream {index:Option<u64>,partial:String,emitted:String}
fn field(value:&Value)->Option<&str> {
    let object=value.as_object()?;
    if object.len()!=1 {return None}
    object.get("reply")?.as_str()
}
fn prefix(text:&str)->Option<String> {
    serde_json::from_str::<Value>(text).ok().and_then(|v|field(&v).map(str::to_string))
        .or_else(||serde_json::from_str::<Value>(&format!("{text}\"}}")).ok().and_then(|v|field(&v).map(str::to_string)))
}
impl ReplyStream {
    fn extend(&mut self,reply:&str)->Result<String,String> {
        let delta=reply.strip_prefix(&self.emitted).ok_or("Formatted reply changed already streamed text")?.to_string();
        self.emitted=reply.to_string();Ok(delta)
    }
    pub fn observe(&mut self,frame:&Value)->Result<Option<String>,String> {
        if frame.get("parent_tool_use_id").is_some_and(|v|!v.is_null()) {return Ok(None)}
        if frame["type"]=="stream_event" {
            let event=&frame["event"];
            if event["type"]=="message_start" {self.index=None;self.partial.clear();}
            if event["type"]=="content_block_start" && event["content_block"]["type"]=="tool_use" && event["content_block"]["name"]=="StructuredOutput" {
                self.index=event["index"].as_u64();self.partial.clear();
            }
            if event["type"]=="content_block_delta" && self.index.is_some() && event["index"].as_u64()==self.index && event["delta"]["type"]=="input_json_delta" {
                self.partial.push_str(event["delta"]["partial_json"].as_str().ok_or("Missing formatted reply bytes")?);
                if self.partial.len()>1024*1024 {return Err("Formatted reply exceeds stream limit".into())}
                return Ok(Some(match prefix(&self.partial) {Some(text)=>self.extend(&text)?,None=>String::new()}));
            }
        }
        if frame["type"]=="assistant" {
            for block in frame["message"]["content"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
                if block["type"]=="tool_use" && block["name"]=="StructuredOutput" {
                    return self.extend(field(&block["input"]).ok_or("Invalid formatted reply")?).map(Some)
                }
            }
        }
        Ok(None)
    }
    pub fn finish(&mut self,result:&Value)->Result<String,String> {
        self.extend(field(&result["structured_output"]).ok_or("Provider omitted the formatted reply")?)
    }
}
/// Native formatting completes locally even when the model stopped at tool_use.
pub fn completed(result:&Value)->bool {
    result["is_error"]==false && result["subtype"]=="success" && field(&result["structured_output"]).is_some()
}

pub fn schema()->Value {
    serde_json::json!({"type":"object","properties":{"reply":{"type":"string",
        "description":"The reply addressed directly to the user, or an empty string when no reply is needed."}},"required":["reply"],"additionalProperties":false})
}

#[cfg(test)]
mod tests {
 use super::*;
 use serde_json::json;
 #[test]
 fn every_character_split_preserves_json_escapes_and_never_promotes_plain_commentary() {
  for text in ["Hello, user.","A \"quote\" and \\ slash\nsecond line","Emoji 😀 and café.","Literal } and { and \\\"reply\\\".","", "Still working on: 日本語"] {
   let input=json!({"reply":text}).to_string();
   for split in input.char_indices().map(|(i,_)|i).chain(std::iter::once(input.len())) {
    let mut stream=ReplyStream::default();let mut delivered=String::new();
    assert!(stream.observe(&json!({"type":"assistant","message":{"content":[{"type":"text","text":"Just acknowledge. This is commentary."}]}})).unwrap().is_none());
    stream.observe(&json!({"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","name":"StructuredOutput"}}})).unwrap();
    for piece in [&input[..split],&input[split..]] {delivered.push_str(&stream.observe(&json!({"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":piece}}})).unwrap().unwrap());}
    delivered.push_str(&stream.finish(&json!({"structured_output":{"reply":text}})).unwrap());assert_eq!(delivered,text);
   }
  }
  for value in [json!(null),json!({"text":"not a reply"}),json!({"reply":"ok","other":"forbidden"})] {assert!(ReplyStream::default().finish(&json!({"structured_output":value})).is_err());}
  assert!(completed(&json!({"is_error":false,"subtype":"success","structured_output":{"reply":""}})),"choosing silence is a completed reply, not a provider failure");
 }
 #[test]
 fn unicode_surrogates_nested_agents_and_rewrites_cannot_leak_into_reply() {
  assert!(prefix("{\"reply\":\"\\uD83D").is_none());assert_eq!(prefix("{\"reply\":\"\\uD83D\\uDE00"),Some("😀".into()));
  let mut stream=ReplyStream::default();assert!(stream.observe(&json!({"type":"assistant","parent_tool_use_id":"worker","message":{"content":[{"type":"tool_use","name":"StructuredOutput","input":{"reply":"worker prose"}}]}})).unwrap().is_none());
  assert_eq!(stream.finish(&json!({"structured_output":{"reply":"Hello"}})).unwrap(),"Hello");assert!(stream.finish(&json!({"structured_output":{"reply":"Different"}})).is_err());
 }
}

