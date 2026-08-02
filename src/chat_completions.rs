// https://api-docs.deepseek.com/zh-cn/api/create-chat-completion

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub messages: Vec<Message>,
    pub model: String,
    // thinking: Option<Thinking>,
    // reasoning_effort: Option<ReasoningEffort>,
    // max_tokens: Option<i64>,
    // response_format: Option<ResponseFormat>
    // stop,
    // stream,
    // stream_options,
    // temperature,
    // top_p
    // tools
    // tool_choice
    // logprobs
    // top_logprobs,
    // user_id
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    System {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    User {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    // Assistant {
    //     content: String,
    //     #[serde(skip_serializing_if = "Option::is_none")]
    //     name: Option<String>,
    //     // prefix: Bool,
    //     // reasoning_content: String,
    // },
    // Tool {
    //     content: String,
    //     tool_call_id: String,
    // },
}

// enum ReasoningEffort {
//     low,
//     high,
//     max,
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    id: String,
    choices: Vec<Choice>,
    created: i64,
    model: String,
    system_fingerprint: String,
    object: String,
    // usage: Usage,
}

#[derive(Debug, Serialize, Deserialize)]
struct Choice {
    finish_reason: FinishReason,
    index: i64,
    message: ResponseMessage,
    // logprobs: Option<Logprobs>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    InsufficientSystemResource,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    role: Role,
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_call_type: ToolCallType,
    function: Function,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ToolCallType {
    Function,
}

#[derive(Debug, Serialize, Deserialize)]
struct Function {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Assistant,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> Request {
        Request {
            model: "deepseek-chat".to_string(),
            messages: vec![
                Message::System {
                    content: "You are a helpful assistant.".to_string(),
                    name: None,
                },
                Message::User {
                    content: "Hello!".to_string(),
                    name: None,
                },
            ],
        }
    }

    // role tag + snake_case 改名是否生效；model 是否带上
    #[test]
    fn serialize_message_uses_role_tag_and_snake_case() {
        let json = serde_json::to_string(&sample_request()).unwrap();
        assert!(json.contains(r#""role":"system""#), "got: {json}");
        assert!(json.contains(r#""role":"user""#), "got: {json}");
        assert!(json.contains(r#""model":"deepseek-chat""#), "got: {json}");
    }

    // name 为 None 时整键不应出现（skip_serializing_if 生效）
    #[test]
    fn serialize_omits_name_when_none() {
        let json = serde_json::to_string(&sample_request()).unwrap();
        assert!(!json.contains(r#""name""#), "got: {json}");
    }

    // name 为 Some 时应输出
    #[test]
    fn serialize_keeps_name_when_some() {
        let req = Request {
            model: "deepseek-chat".to_string(),
            messages: vec![Message::User {
                content: "Hi".to_string(),
                name: Some("alice".to_string()),
            }],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""name":"alice""#), "got: {json}");
    }

    // 序列化后能否无损反序列化回相同结构（往返）
    #[test]
    fn serialize_then_parse_roundtrips_role() {
        let json = serde_json::to_string(&sample_request()).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, "deepseek-chat");
        assert_eq!(parsed.messages.len(), 2);
        assert!(matches!(parsed.messages[0], Message::System { .. }));
        assert!(matches!(parsed.messages[1], Message::User { .. }));
    }

    // 解析典型的 DeepSeek 响应，覆盖 Response / Choice / ResponseMessage / Role
    #[test]
    fn deserialize_full_response() {
        let raw = r#"{
            "id": "chatcmpl-1",
            "choices": [
                {
                    "finish_reason": "stop",
                    "index": 0,
                    "message": {
                        "content": "hello",
                        "role": "assistant"
                    }
                }
            ],
            "created": 1700000000,
            "model": "deepseek-chat",
            "system_fingerprint": "fp_1",
            "object": "chat.completion"
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.id, "chatcmpl-1");
        assert_eq!(resp.model, "deepseek-chat");
        assert_eq!(resp.object, "chat.completion");
        assert_eq!(resp.created, 1700000000);
        assert_eq!(resp.choices.len(), 1);
        let choice = &resp.choices[0];
        assert!(matches!(choice.finish_reason, FinishReason::Stop));
        assert_eq!(choice.index, 0);
        assert_eq!(choice.message.content.as_deref(), Some("hello"));
        assert!(matches!(choice.message.role, Role::Assistant));
    }

    // content/reasoning_content/tool_calls 缺失也能解析（API 可能不返回）
    #[test]
    fn deserialize_message_with_optional_fields_absent() {
        let raw = r#"{
            "id": "x",
            "choices": [
                {
                    "finish_reason": "length",
                    "index": 1,
                    "message": { "role": "assistant" }
                }
            ],
            "created": 1,
            "model": "m",
            "system_fingerprint": "",
            "object": "chat.completion"
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.content.is_none());
        assert!(msg.reasoning_content.is_none());
        assert!(msg.tool_calls.is_none());
        assert!(matches!(resp.choices[0].finish_reason, FinishReason::Length));
    }
}
