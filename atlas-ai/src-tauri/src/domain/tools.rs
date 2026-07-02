use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolCallStatus {
    Pending,
    Denied,
    Succeeded,
    Failed,
}

impl ToolCallStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ToolCallStatus::Pending => "pending",
            ToolCallStatus::Denied => "denied",
            ToolCallStatus::Succeeded => "succeeded",
            ToolCallStatus::Failed => "failed",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "pending" => Ok(ToolCallStatus::Pending),
            "denied" => Ok(ToolCallStatus::Denied),
            "succeeded" => Ok(ToolCallStatus::Succeeded),
            "failed" => Ok(ToolCallStatus::Failed),
            _ => Err(format!("Unknown tool call status: {value}")),
        }
    }
}

impl fmt::Display for ToolCallStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolPermissionDecision {
    AllowOnce,
    AlwaysAllowWorkspace,
    Deny,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolPermissionScopeType {
    Workspace,
}

impl ToolPermissionScopeType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ToolPermissionScopeType::Workspace => "workspace",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "workspace" => Ok(ToolPermissionScopeType::Workspace),
            _ => Err(format!("Unknown tool permission scope: {value}")),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ToolCall {
    pub(crate) id: String,
    pub(crate) conversation_id: String,
    pub(crate) message_id: Option<i64>,
    pub(crate) tool_name: String,
    pub(crate) arguments_json: String,
    pub(crate) arguments_summary: String,
    pub(crate) status: ToolCallStatus,
    pub(crate) result_summary: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) completed_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub(crate) struct ToolCallInsert<'a> {
    pub(crate) conversation_id: &'a str,
    pub(crate) message_id: Option<i64>,
    pub(crate) tool_name: &'a str,
    pub(crate) arguments_json: &'a str,
    pub(crate) arguments_summary: &'a str,
    pub(crate) created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ToolPermission {
    pub(crate) id: String,
    pub(crate) scope_type: ToolPermissionScopeType,
    pub(crate) scope_id: String,
    pub(crate) tool_name: String,
    pub(crate) permission: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}
