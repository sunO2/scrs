use regex::Regex;
use tracing::{debug, warn};
use crate::agent::core::traits::ParsedAction;

/// 从 LLM 响应中解析操作
pub fn parse_action_from_response(response: &str) -> Result<Option<ParsedAction>, crate::agent::core::traits::ModelError> {
    debug!("🔍 开始解析 LLM 响应，长度: {} 字符", response.len());
    debug!("📝 响应内容: {}", response);

    // 1. 尝试解析 do(action=...) 格式（Python 风格的函数调用）
    debug!("🔄 [1/5] 尝试解析 do(action=...) 格式");
    if let Some(action) = try_parse_do_action(response) {
        debug!("✅ 成功解析 do(action=...) 格式: action_type={}, parameters={}",
               action.action_type, action.parameters);
        return Ok(Some(action));
    }
    debug!("❌ 未找到 do(action=...) 格式");

    // 2. 尝试解析 finish(message=...) 格式
    debug!("🔄 [2/5] 尝试解析 finish(message=...) 格式");
    if let Some(action) = try_parse_finish_action(response) {
        debug!("✅ 成功解析 finish(message=...) 格式: message={:?}",
               action.parameters.get("result"));
        return Ok(Some(action));
    }
    debug!("❌ 未找到 finish(message=...) 格式");

    // 3. 尝试解析 JSON 格式的操作
    debug!("🔄 [3/5] 尝试解析 JSON 格式");
    if let Some(action) = try_parse_json_action(response) {
        debug!("✅ 成功解析 JSON 格式: action_type={}, parameters={}",
               action.action_type, action.parameters);
        return Ok(Some(action));
    }
    debug!("❌ 未找到 JSON 格式");

    // // 4. 尝试解析特定格式的文本（Launch、tap 等）
    // debug!("🔄 [4/5] 尝试解析文本格式 (Launch, Tap, Swipe 等)");
    // if let Some(action) = try_parse_text_action(response) {
    //     debug!("✅ 成功解析文本格式: action_type={}, parameters={}",
    //            action.action_type, action.parameters);
    //     return Ok(Some(action));
    // }
    // debug!("❌ 未找到文本格式");

    // 5. 如果响应包含 "finish" 或 "done"，表示任务完成
    debug!("🔄 [5/5] 检查是否包含完成关键词 (finish/done/complete)");
    if response.to_lowercase().contains("finish")
        || response.to_lowercase().contains("done")
        || response.to_lowercase().contains("complete")
    {
        // 提取完成消息
        let result = extract_completion_message(response);
        debug!("✅ 检测到完成关键词，提取消息: {}", result);
        return Ok(Some(ParsedAction {
            action_type: "finish".to_string(),
            parameters: serde_json::json!({
                "result": result,
                "success": true
            }),
            reasoning: response.to_string(),
        }));
    }
    debug!("❌ 未找到完成关键词");

    // 如果无法解析操作，返回 None（可能只是思考过程）
    debug!("⚠️  所有解析方式均失败，返回 None（可能是纯思考内容）");
    Ok(None)
}

/// 尝试解析 do(action=...) 格式
/// 支持: do(action="Launch", app="微信") 或 do(action="Tap", element=[500, 800])
pub fn try_parse_do_action(response: &str) -> Option<ParsedAction> {
    debug!("  📌 尝试匹配 do(...) 正则表达式");
    // 查找 do( ... ) 格式
    let do_regex = Regex::new(r#"do\s*\(([^)]+)\)"#).ok()?;

    if let Some(caps) = do_regex.captures(response) {
        let params_str = caps.get(1)?.as_str();
        debug!("  ✅ 匹配到 do(...) 格式，参数字符串: {}", params_str);

        // 解析参数
        let mut action_type = None;
        let mut params = serde_json::Map::new();

        debug!("  📋 开始解析参数...");

        // 首先匹配 key="value" 格式（带引号）
        let quoted_param_regex = Regex::new(r#"(\w+)\s*=\s*["']([^"']+)["']"#).ok()?;
        let mut parsed_keys = std::collections::HashSet::new();

        for param_caps in quoted_param_regex.captures_iter(params_str) {
            let key = param_caps.get(1)?.as_str();
            let value = param_caps.get(2)?.as_str();
            parsed_keys.insert(key.to_string());
            debug!("    参数（引号）: {} = {}", key, value);

            if key == "action" {
                action_type = Some(value.to_string());
                debug!("    🎯 找到 action 类型: {}", value);
            } else {
                params.insert(key.to_string(), serde_json::json!(value));
            }
        }

        // 然后匹配 key=value 格式（不带引号，如 element=[500, 800]）
        // 排除已经解析的键
        for (key, value) in parse_key_value_pairs(params_str) {
            if !parsed_keys.contains(&key) {
                debug!("    参数（无引号）: {} = {}", key, value);
                if key == "action" {
                    if action_type.is_none() {
                        action_type = Some(value.clone());
                        debug!("    🎯 找到 action 类型: {}", value);
                    }
                } else {
                    // 尝试解析为 JSON（数组、数字等）
                    let parsed_value = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&value) {
                        json_val
                    } else {
                        // 如果不是 JSON，当作字符串
                        serde_json::json!(value)
                    };
                    params.insert(key, parsed_value);
                }
            }
        }

        if let Some(action_type) = action_type {
            // 转换操作类型名称
            let normalized_type = normalize_action_type(&action_type);
            debug!("  🔄 标准化操作类型: {} -> {}", action_type, normalized_type);
            debug!("  📦 最终参数: {}", serde_json::to_string(&params).unwrap_or_else(|_| "Invalid".to_string()));

            return Some(ParsedAction {
                action_type: normalized_type,
                parameters: serde_json::Value::Object(params),
                reasoning: response.to_string(),
            });
        } else {
            debug!("  ❌ 未能提取 action 类型");
        }
    } else {
        debug!("  ❌ 未匹配到 do(...) 格式");
    }

    None
}

/// 解析 key=value 格式的参数（支持无引号的值，如 element=[500, 800]）
fn parse_key_value_pairs(params_str: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let current = params_str.trim();
    let mut in_brackets = 0;
    let mut start = 0;

    for (i, c) in current.char_indices() {
        match c {
            '[' | '{' | '(' => in_brackets += 1,
            ']' | '}' | ')' => in_brackets -= 1,
            ',' if in_brackets == 0 => {
                let pair = &current[start..i].trim();
                if let Some((key, value)) = parse_single_pair(pair) {
                    result.push((key, value));
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    // 处理最后一个参数
    let last_pair = &current[start..].trim();
    if let Some((key, value)) = parse_single_pair(last_pair) {
        result.push((key, value));
    }

    result
}

/// 解析单个 key=value 对
fn parse_single_pair(pair: &str) -> Option<(String, String)> {
    let pair = pair.trim();
    if let Some(eq_pos) = pair.find('=') {
        let key = pair[..eq_pos].trim().to_string();
        let value = pair[eq_pos + 1..].trim().to_string();
        Some((key, value))
    } else {
        None
    }
}

/// 尝试解析 finish(message=...) 格式
pub fn try_parse_finish_action(response: &str) -> Option<ParsedAction> {
    debug!("  🏁 尝试匹配 finish(message=...) 正则表达式");

    // 首先尝试匹配 finish(message="xxx") 格式（带引号）
    if let Some(start) = response.find("finish(message=\"") {
        let start_pos = start + 16; // "finish(message=\"" 的长度
        if let Some(end) = response[start_pos..].find("\")") {
            let message = &response[start_pos..start_pos + end];
            debug!("  ✅ 匹配到 finish(message=\"...\") 格式，消息: {}", message);
            return Some(ParsedAction {
                action_type: "finish".to_string(),
                parameters: serde_json::json!({
                    "result": message,
                    "success": true
                }),
                reasoning: response.to_string(),
            });
        }
    }

    // 然后尝试匹配 finish(message=xxx) 格式（不带引号，到下一个)或行尾）
    if let Some(start) = response.find("finish(message=") {
        let start_pos = start + 14; // "finish(message=" 的长度
        let remaining = &response[start_pos..];

        // 查找结束位置：")" 或行尾
        let end_pos = remaining.find(')')
            .or_else(|| remaining.find('\n').map(|pos| pos))
            .unwrap_or(remaining.len());

        let message = remaining[..end_pos].trim();
        debug!("  ✅ 匹配到 finish(message=...) 格式，消息: {}", message);
        return Some(ParsedAction {
            action_type: "finish".to_string(),
            parameters: serde_json::json!({
                "result": message,
                "success": true
            }),
            reasoning: response.to_string(),
        });
    }

    // 最后尝试简单的 finish("xxx") 格式
    if let Some(start) = response.find("finish(\"") {
        let start_pos = start + 8; // "finish(\"" 的长度
        if let Some(end) = response[start_pos..].find("\")") {
            let message = &response[start_pos..start_pos + end];
            debug!("  ✅ 匹配到 finish(\"...\") 格式，消息: {}", message);
            return Some(ParsedAction {
                action_type: "finish".to_string(),
                parameters: serde_json::json!({
                    "result": message,
                    "success": true
                }),
                reasoning: response.to_string(),
            });
        }
    }

    debug!("  ❌ 未匹配到 finish(message=...) 格式");
    None
}

/// 标准化操作类型名称
/// 将 "Launch" 转换为 "launch"，"Tap" 转换为 "tap" 等
fn normalize_action_type(action_type: &str) -> String {
    match action_type.to_lowercase().as_str() {
        "launch" => String::from("launch"),
        "tap" => String::from("tap"),
        "double_tap" | "doubletap" => String::from("double_tap"),
        "long_press" | "longpress" => String::from("long_press"),
        "swipe" => String::from("swipe"),
        "scroll" => String::from("scroll"),
        "type" | "type_name" => String::from("type"),
        "press_key" | "presskey" => String::from("press_key"),
        "back" => String::from("back"),
        "home" => String::from("home"),
        "recent" => String::from("recent"),
        "notification" => String::from("notification"),
        "wait" => String::from("wait"),
        "screenshot" => String::from("screenshot"),
        "finish" => String::from("finish"),
        _ => action_type.to_lowercase(),
    }
}

/// 尝试解析 JSON 格式的操作
fn try_parse_json_action(response: &str) -> Option<ParsedAction> {
    debug!("  📋 尝试解析 JSON 格式");
    // 查找 JSON 块
    let json_regex = Regex::new(r"\{[^{}]*\}").ok()?;
    let json_captures: Vec<_> = json_regex.find_iter(response).collect();

    debug!("  🔍 找到 {} 个可能的 JSON 块", json_captures.len());
    for (idx, json_match) in json_captures.iter().enumerate() {
        let json_str = json_match.as_str();
        debug!("    尝试解析 JSON [{}]: {}", idx, json_str);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
            if let Some(action_type) = json.get("action_type")
                .or(json.get("type"))
                .or(json.get("action"))
                .and_then(|v| v.as_str())
            {
                debug!("  ✅ 成功解析 JSON，action_type: {}", action_type);
                return Some(ParsedAction {
                    action_type: action_type.to_string(),
                    parameters: json,
                    reasoning: response.to_string(),
                });
            } else {
                debug!("  ⚠️  JSON 有效但未找到 action_type 字段");
            }
        } else {
            debug!("  ⚠️  JSON 解析失败");
        }
    }

    debug!("  ❌ 未找到有效的 JSON 操作");
    None
}

/// 尝试解析文本格式的操作
// fn try_parse_text_action(response: &str) -> Option<ParsedAction> {
//     debug!("  📝 尝试解析文本格式");
//     let response_lower = response.to_lowercase();

//     // Launch 操作: "Launch(\"微信\")" 或 "Launch(微信)" 或 "启动应用:微信"
//     if response_lower.contains("launch") || response_lower.contains("启动") {
//         debug!("    🔍 检测到 Launch 关键词");
//         // 尝试解析 Launch("app_name") 格式
//         if let Some(app_name) = extract_launch_app(response) {
//             debug!("  ✅ 解析到 Launch 操作: {}", app_name);
//             return Some(ParsedAction {
//                 action_type: "launch".to_string(),
//                 parameters: serde_json::json!({
//                     "app_name": app_name
//                 }),
//                 reasoning: response.to_string(),
//             });
//         }
//     }

//     // 点击操作: "tap at (100, 200)" 或 "点击 (100, 200)"
//     if response_lower.contains("tap") || response_lower.contains("点击") {
//         debug!("    🔍 检测到 Tap 关键词");
//         if let Some(coords) = extract_coordinates(response) {
//             debug!("  ✅ 解析到 Tap 操作: ({}, {})", coords.0, coords.1);
//             return Some(ParsedAction {
//                 action_type: "tap".to_string(),
//                 parameters: serde_json::json!({
//                     "x": coords.0,
//                     "y": coords.1
//                 }),
//                 reasoning: response.to_string(),
//             });
//         }
//     }

//     // 滑动操作: "swipe from (100, 200) to (300, 400)" 或 "滑动从 (100, 200) 到 (300, 400)"
//     if response_lower.contains("swipe") || response_lower.contains("滑动") {
//         debug!("    🔍 检测到 Swipe 关键词");
//         if let (Some(start), Some(end)) = (extract_coordinates(response), extract_coordinates(response)) {
//             debug!("  ✅ 解析到 Swipe 操作: ({}, {}) -> ({}, {})", start.0, start.1, end.0, end.1);
//             return Some(ParsedAction {
//                 action_type: "swipe".to_string(),
//                 parameters: serde_json::json!({
//                     "start_x": start.0,
//                     "start_y": start.1,
//                     "end_x": end.0,
//                     "end_y": end.1,
//                     "duration_ms": 500
//                 }),
//                 reasoning: response.to_string(),
//             });
//         }
//     }

//     // 输入操作: "type: hello" 或 "输入: hello"
//     if response_lower.contains("type:") || response_lower.contains("输入:") {
//         debug!("    🔍 检测到 Type 关键词");
//         if let Some(text) = extract_text_after(response, &["type:", "输入:", "input:"]) {
//             debug!("  ✅ 解析到 Type 操作: {}", text);
//             return Some(ParsedAction {
//                 action_type: "type".to_string(),
//                 parameters: serde_json::json!({
//                     "text": text
//                 }),
//                 reasoning: response.to_string(),
//             });
//         }
//     }

//     // 返回操作: "back" 或 "返回"
//     if response_lower.contains("back") || response_lower.contains("返回") {
//         debug!("  ✅ 解析到 Back 操作");
//         return Some(ParsedAction {
//             action_type: "back".to_string(),
//             parameters: serde_json::json!({}),
//             reasoning: response.to_string(),
//         });
//     }

//     // Home 操作: "home" 或 "主页"
//     if response_lower.contains("home") || response_lower.contains("主页") {
//         debug!("  ✅ 解析到 Home 操作");
//         return Some(ParsedAction {
//             action_type: "home".to_string(),
//             parameters: serde_json::json!({}),
//             reasoning: response.to_string(),
//         });
//     }

//     // 等待操作: "wait 1s" 或 "等待 1 秒"
//     if response_lower.contains("wait") || response_lower.contains("等待") {
//         debug!("    🔍 检测到 Wait 关键词");
//         if let Some(duration) = extract_duration(response) {
//             debug!("  ✅ 解析到 Wait 操作: {}ms", duration);
//             return Some(ParsedAction {
//                 action_type: "wait".to_string(),
//                 parameters: serde_json::json!({
//                     "duration_ms": duration
//                 }),
//                 reasoning: response.to_string(),
//             });
//         }
//     }

//     debug!("  ❌ 未找到任何文本格式的操作");
//     None
// }

/// 提取坐标
fn extract_coordinates(text: &str) -> Option<(u32, u32)> {
    let coord_regex = Regex::new(r"\((\d+)[\s,]+(\d+)\)").ok()?;
    if let Some(caps) = coord_regex.captures(text) {
        let x = caps.get(1)?.as_str().parse().ok()?;
        let y = caps.get(2)?.as_str().parse().ok()?;
        return Some((x, y));
    }
    None
}

/// 提取 Launch 操作中的应用名称
/// 支持 "Launch(\"微信\")" 和 "Launch(微信)" 等格式
fn extract_launch_app(text: &str) -> Option<String> {
    // 尝试匹配 Launch("app_name") 或 Launch(app_name)
    let launch_regex = Regex::new(r#"(?i)Launch\s*\(\s*["']?([^"')]+)["']?\s*\)"#).ok()?;

    if let Some(caps) = launch_regex.captures(text) {
        let app_name = caps.get(1)?.as_str().trim();
        return Some(app_name.to_string());
    }

    // 尝试匹配 "启动应用:微信" 或 "启动:微信" 格式
    if let Some(pos) = text.to_lowercase().find("启动") {
        let after_launch = &text[pos + 6..]; // 6 = "启动".len()
        if let Some(colon_pos) = after_launch.find(':') {
            let app_name = after_launch[colon_pos + 1..].trim();
            if !app_name.is_empty() {
                return Some(app_name.to_string());
            }
        }
    }

    None
}

/// 提取指定关键词后的文本
fn extract_text_after(text: &str, keywords: &[&str]) -> Option<String> {
    for keyword in keywords {
        if let Some(pos) = text.find(keyword) {
            let after = &text[pos + keyword.len()..];
            let trimmed = after.trim();
            if !trimmed.is_empty() {
                // 只取第一行或第一个句子
                let result = trimmed
                    .split('\n')
                    .next()
                    .or_else(|| trimmed.split('.').next())
                    .unwrap_or(trimmed)
                    .trim();
                return Some(result.to_string());
            }
        }
    }
    None
}

/// 提取持续时间（毫秒）
fn extract_duration(text: &str) -> Option<u32> {
    let text_lower = text.to_lowercase();

    // 尝试解析 "wait 1s", "等待 2 秒" 等格式
    if let Some(caps) = Regex::new(r"(\d+)\s*(s|sec|second|秒)").ok()?.captures(&text_lower) {
        let seconds: u32 = caps.get(1)?.as_str().parse().ok()?;
        return Some(seconds * 1000);
    }

    // 尝试解析 "wait 1000ms", "等待 500 毫秒" 等格式
    if let Some(caps) = Regex::new(r"(\d+)\s*(ms|millis|millisecond|毫秒)").ok()?.captures(&text_lower) {
        let millis: u32 = caps.get(1)?.as_str().parse().ok()?;
        return Some(millis);
    }

    None
}

/// 提取完成消息
fn extract_completion_message(text: &str) -> String {
    // 提取引号中的内容或整个文本的前 100 个字符
    if let Ok(re) = Regex::new(r#""([^"]+)""#) {
        if let Some(caps) = re.captures(text) {
            if let Some(m) = caps.get(1) {
                return m.as_str().to_string();
            }
        }
    }

    // 截取前 100 个字符
    let trimmed = text.trim();
    if trimmed.len() > 100 {
        format!("{}...", &trimmed[..100])
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_coordinates() {
        assert_eq!(extract_coordinates("tap at (100, 200)"), Some((100, 200)));
        assert_eq!(extract_coordinates("点击 (500, 800)"), Some((500, 800)));
    }

    #[test]
    fn test_extract_duration() {
        assert_eq!(extract_duration("wait 1s"), Some(1000));
        assert_eq!(extract_duration("等待 2 秒"), Some(2000));
        assert_eq!(extract_duration("wait 500ms"), Some(500));
    }

    #[test]
    fn test_parse_json_action() {
        let response = r#"{"action_type": "tap", "x": 100, "y": 200}"#;
        let action = try_parse_json_action(response);
        assert!(action.is_some());
        assert_eq!(action.unwrap().action_type, "tap");
    }

    #[test]
    fn test_extract_launch_app() {
        assert_eq!(extract_launch_app("Launch(\"微信\")"), Some("微信".to_string()));
        assert_eq!(extract_launch_app("Launch(微信)"), Some("微信".to_string()));
        assert_eq!(extract_launch_app("launch \"weixin\""), Some("weixin".to_string()));
    }

    // #[test]
    // fn test_parse_launch_action() {
    //     let response = "我应该使用Launch功能直接启动微信应用。Launch(\"微信\")";
    //     let action = try_parse_text_action(response);
    //     assert!(action.is_some());
    //     assert_eq!(action.unwrap().action_type, "launch");
    // }

    #[test]
    fn test_parse_do_action_launch() {
        let response = r#"do(action="Launch", app="微信")"#;
        let action = try_parse_do_action(response);
        assert!(action.is_some());
        let parsed = action.unwrap();
        assert_eq!(parsed.action_type, "launch");
        assert_eq!(parsed.parameters.get("app"), Some(&serde_json::json!("微信")));
    }

    #[test]
    fn test_parse_do_action_tap() {
        let response = r#"do(action="Tap", element=[500, 800])"#;
        let action = try_parse_do_action(response);
        assert!(action.is_some());
        let parsed = action.unwrap();
        assert_eq!(parsed.action_type, "tap");
        assert_eq!(parsed.parameters.get("element"), Some(&serde_json::json!([500, 800])));
    }

    #[test]
    fn test_parse_finish_action() {
        let response = r#"finish(message="任务完成")"#;
        let action = try_parse_finish_action(response);
        assert!(action.is_some());
        let parsed = action.unwrap();
        assert_eq!(parsed.action_type, "finish");
    }

    #[test]
    fn test_normalize_action_type() {
        assert_eq!(normalize_action_type("Launch"), "launch");
        assert_eq!(normalize_action_type("Tap"), "tap");
        assert_eq!(normalize_action_type("DoubleTap"), "double_tap");
        assert_eq!(normalize_action_type("LongPress"), "long_press");
    }
}
