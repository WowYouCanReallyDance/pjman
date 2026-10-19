use std::sync::LazyLock;

use uuid::Uuid;

/// 内置默认虚拟 workspace（DefaultVirtualWorkspace）的固定 id
pub static DVWS_ID: LazyLock<String> =
    LazyLock::new(|| "00000000-0000-7000-0000-000000000001".to_owned());

/// 内置默认虚拟 workspace 的固定名称
pub const DVWS_NAME: &str = "dvws";

/// 生成新的 uuidv7（文本形式，跨数据库可移植）
#[allow(dead_code)]
pub fn new_id() -> String {
    Uuid::now_v7().to_string()
}
