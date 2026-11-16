import { useEffect, useState } from "react";
import { api, type WebhookChannel } from "../lib/tauri";

const platforms = [
  { platform: "wecom", name: "企业微信", placeholder: "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=…" },
  { platform: "feishu", name: "飞书", placeholder: "https://open.feishu.cn/open-apis/bot/v2/hook/…" },
  { platform: "dingtalk", name: "钉钉", placeholder: "https://oapi.dingtalk.com/robot/send?access_token=…" },
] as const;

export default function WebhookSettings() {
  const [channels, setChannels] = useState<WebhookChannel[]>(platforms.map(({ platform }) => ({ platform, enabled: false, url: "", secret: "" })));
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    api.getWebhookConfig().then((saved) => {
      setChannels((defaults) => defaults.map((c) => saved.find((s) => s.platform === c.platform) ?? c));
      setLoaded(true);
    }).catch((e) => setMessage(`读取失败：${e}`));
  }, []);
  const update = (index: number, patch: Partial<WebhookChannel>) => {
    setChannels((previous) => previous.map((c, i) => i === index ? { ...c, ...patch } : c));
    setDirty(true);
    setMessage("");
  };
  const run = async (index?: number) => {
    setBusy(true);
    setMessage(index === undefined ? "正在保存…" : "正在发送测试消息…");
    try {
      if (index === undefined) {
        await api.saveWebhookConfig(channels);
        setDirty(false);
        setMessage("通知配置已保存");
      } else {
        await api.testWebhook(channels[index]);
        setMessage(`${platforms[index].name}测试发送成功${dirty ? "，配置尚未保存" : ""}`);
      }
    } catch (e) {
      setMessage(`操作失败：${e}`);
    } finally { setBusy(false); }
  };
  return <section className="settings-group webhook-settings">
    <div className="settings-row"><span className="settings-label">群机器人通知</span><span className="settings-hint">{dirty ? "有未保存更改" : "任务到期时发送"}</span></div>
    <p className="settings-hint">可同时启用三个平台，包含错过的提醒。地址和密钥保存在本机设置文件中。机器人关键词可设为「weldd」。</p>
    {platforms.map((p, index) => <fieldset key={p.platform} disabled={busy || !loaded} className="webhook-channel">
      <legend>{p.name}</legend>
      <label className="settings-row"><span className="settings-label">启用通知</span><input aria-label={`启用${p.name}通知`} className="settings-toggle" type="checkbox" checked={channels[index].enabled} onChange={(e) => update(index, { enabled: e.target.checked })} /></label>
      <label className="webhook-field">Webhook 地址<input type="password" autoComplete="off" spellCheck={false} placeholder={p.placeholder} value={channels[index].url} onChange={(e) => update(index, { url: e.target.value })} /></label>
      {p.platform !== "wecom" && <label className="webhook-field">签名密钥（开启加签时填写）<input type="password" autoComplete="off" value={channels[index].secret} placeholder="未开启加签可留空" onChange={(e) => update(index, { secret: e.target.value })} /></label>}
      <button type="button" className="settings-btn" disabled={!channels[index].url.trim()} onClick={() => void run(index)}>发送测试消息</button>
    </fieldset>)}
    <div className="settings-row"><button type="button" className="settings-btn" disabled={busy || !loaded} onClick={() => void run()}>保存通知配置</button></div>
    <p className="webhook-result" role="status">{message}</p>
  </section>;
}
