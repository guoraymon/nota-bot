#[derive(sqlx::FromRow)]
pub struct DbMessage {
    #[allow(dead_code)]
    pub id: i64,
    #[allow(dead_code)]
    pub conversation_id: i64,
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    #[allow(dead_code)]
    pub created_at: i64,
}
