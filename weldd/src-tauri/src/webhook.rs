//! 群机器人通知独立异步发送,网络失败不阻塞本地提醒。
use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Wecom,
    Feishu,
    Dingtalk,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    platform: Platform,
    enabled: bool,
    url: String,
    #[serde(default)]
    secret: String,
}

fn validate(channel: &Channel) -> Result<Url, String> {
    let url = Url::parse(channel.url.trim()).map_err(|_| "Webhook 地址格式不正确")?;
    let valid = match channel.platform {
        Platform::Wecom => {
            url.host_str() == Some("qyapi.weixin.qq.com")
                && url.path() == "/cgi-bin/webhook/send"
                && url.query_pairs().any(|(k, v)| k == "key" && !v.is_empty())
        }
        Platform::Feishu => {
            url.host_str() == Some("open.feishu.cn")
                && url.path().starts_with("/open-apis/bot/v2/hook/")
                && url.path().len() > 23
        }
        Platform::Dingtalk => {
            url.host_str() == Some("oapi.dingtalk.com")
                && url.path() == "/robot/send"
                && url
                    .query_pairs()
                    .any(|(k, v)| k == "access_token" && !v.is_empty())
        }
    };
    if !valid
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err("请填写所选平台的官方 HTTPS 群机器人 Webhook 地址".into());
    }
    if channel.url.len() > 4096 || channel.secret.len() > 512 {
        return Err("Webhook 地址或签名密钥过长".into());
    }
    Ok(url)
}

fn sign(key: &str, content: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC 支持任意长度密钥");
    mac.update(content.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}

fn request(channel: &Channel, content: &str, millis: i64) -> Result<(Url, Value), String> {
    let mut url = validate(channel)?;
    let mut body = match channel.platform {
        Platform::Feishu => json!({"msg_type":"text", "content":{"text":content}}),
        _ => json!({"msgtype":"text", "text":{"content":content}}),
    };
    let secret = channel.secret.trim();
    if !secret.is_empty() {
        match channel.platform {
            Platform::Feishu => {
                let timestamp = (millis / 1000).to_string();
                body["sign"] = json!(sign(&format!("{timestamp}\n{secret}"), ""));
                body["timestamp"] = json!(timestamp);
            }
            Platform::Dingtalk => {
                let timestamp = millis.to_string();
                let signature = sign(secret, &format!("{timestamp}\n{secret}"));
                let pairs: Vec<(String, String)> = url
                    .query_pairs()
                    .filter(|(k, _)| k != "sign" && k != "timestamp")
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect();
                url.set_query(None);
                url.query_pairs_mut()
                    .extend_pairs(pairs)
                    .append_pair("timestamp", &timestamp)
                    .append_pair("sign", &signature);
            }
            Platform::Wecom => {}
        }
    }
    Ok((url, body))
}

fn check_response(platform: &Platform, body: &Value) -> Result<(), String> {
    let code = match platform {
        Platform::Feishu => body.get("code").or_else(|| body.get("StatusCode")),
        _ => body.get("errcode"),
    }
    .and_then(Value::as_i64)
    .ok_or("机器人返回了无法识别的响应")?;
    if code != 0 {
        // 不回显原始响应,防止服务端把令牌或签名带回日志。
        return Err(format!(
            "机器人拒绝消息（错误码 {code}），请检查签名、关键词和 IP 白名单"
        ));
    }
    Ok(())
}

async fn send(channel: &Channel, content: &str) -> Result<(), String> {
    let (url, body) = request(channel, content, chrono::Utc::now().timestamp_millis())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法创建通知客户端")?;
    let response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|_| "发送失败：网络连接异常或超时")?;
    if !response.status().is_success() {
        return Err(format!("发送失败：HTTP {}", response.status().as_u16()));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "机器人响应不是有效 JSON")?;
    check_response(&channel.platform, &body)
}

#[tauri::command]
pub fn get_webhook_config(app: AppHandle) -> Result<Vec<Channel>, String> {
    let store = app
        .store(crate::island::store_file())
        .map_err(|_| "读取通知配置失败")?;
    match store.get("webhook_channels") {
        Some(value) => serde_json::from_value(value).map_err(|_| "通知配置损坏，请重新保存".into()),
        None => Ok(vec![]),
    }
}

#[tauri::command]
pub fn save_webhook_config(app: AppHandle, mut channels: Vec<Channel>) -> Result<(), String> {
    if channels.len() > 3 {
        return Err("最多配置三个平台".into());
    }
    for i in 0..channels.len() {
        if channels[..i]
            .iter()
            .any(|c| c.platform == channels[i].platform)
        {
            return Err("平台不可重复".into());
        }
        channels[i].url = channels[i].url.trim().to_owned();
        channels[i].secret = channels[i].secret.trim().to_owned();
        if channels[i].enabled || !channels[i].url.is_empty() {
            validate(&channels[i])?;
        }
    }
    let store = app
        .store(crate::island::store_file())
        .map_err(|_| "打开通知配置失败")?;
    let previous = store.get("webhook_channels");
    store.set("webhook_channels", json!(channels));
    if store.save().is_err() {
        match previous {
            Some(value) => store.set("webhook_channels", value),
            None => {
                store.delete("webhook_channels");
            }
        }
        return Err("保存通知配置失败".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn test_webhook(channel: Channel) -> Result<(), String> {
    send(
        &channel,
        "weldd 待办提醒：这是一条测试通知，群机器人连接正常。",
    )
    .await
}

pub fn notify(app: &AppHandle, titles: Vec<String>, missed: bool) {
    let channels = match get_webhook_config(app.clone()) {
        Ok(channels) => channels,
        Err(e) => {
            eprintln!("[weldd] {e}");
            return;
        }
    };
    let mut text = format!(
        "weldd 待办提醒：{}，共 {} 项\n",
        if missed {
            "错过的提醒"
        } else {
            "任务到期"
        },
        titles.len()
    );
    for title in &titles {
        let line: String = title.chars().take(120).collect();
        if text.len() + line.len() + 5 > 1800 {
            text.push_str("更多任务请打开 weldd 查看");
            break;
        }
        text.push_str(&format!("• {line}\n"));
    }
    for channel in channels.into_iter().filter(|c| c.enabled) {
        let text = text.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = send(&channel, &text).await {
                eprintln!("[weldd] {:?} 通知失败：{e}", channel.platform);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn channel(platform: Platform, url: &str) -> Channel {
        Channel {
            platform,
            enabled: true,
            url: url.into(),
            secret: "secret".into(),
        }
    }
    #[test]
    fn payload_and_signatures() {
        let c = channel(
            Platform::Feishu,
            "https://open.feishu.cn/open-apis/bot/v2/hook/test",
        );
        let (_, body) = request(&c, "提醒", 1700000000000).unwrap();
        assert_eq!(body["msg_type"], "text");
        assert_eq!(body["timestamp"], "1700000000");
        assert_eq!(body["sign"], "fiWS2+gh28DOydAv7hzONH/mDn9+b1Y4Y5ivXWXy8vA=");
        let c = channel(
            Platform::Dingtalk,
            "https://oapi.dingtalk.com/robot/send?access_token=test&sign=old",
        );
        let (url, body) = request(&c, "提醒", 1700000000000).unwrap();
        assert_eq!(body["text"]["content"], "提醒");
        assert_eq!(url.query_pairs().filter(|(k, _)| k == "sign").count(), 1);
        assert!(url
            .query_pairs()
            .any(|(k, v)| k == "sign" && v == "OuzzJR5+xZ4/EYwqtNt6sMYZQMTa/HEGvc9miJe7XzY="));
        assert!(url
            .query_pairs()
            .any(|(k, v)| k == "timestamp" && v == "1700000000000"));
        let c = channel(
            Platform::Wecom,
            "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=test",
        );
        assert_eq!(
            request(&c, "提醒", 0).unwrap().1,
            json!({"msgtype":"text","text":{"content":"提醒"}})
        );
    }
    #[test]
    fn rejects_invalid_urls_and_provider_errors() {
        for url in [
            "http://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=test",
            "https://example.com/cgi-bin/webhook/send?key=test",
            "https://qyapi.weixin.qq.com/cgi-bin/webhook/send",
        ] {
            assert!(validate(&channel(Platform::Wecom, url)).is_err());
        }
        assert!(check_response(&Platform::Wecom, &json!({"errcode":0})).is_ok());
        assert!(check_response(&Platform::Dingtalk, &json!({"errcode":310000})).is_err());
        assert!(check_response(&Platform::Feishu, &json!({"code":0})).is_ok());
        assert!(check_response(&Platform::Feishu, &json!({"StatusCode":0})).is_ok());
        assert!(check_response(&Platform::Feishu, &json!({"code":19021})).is_err());
        assert!(check_response(&Platform::Wecom, &json!({})).is_err());
    }
}
