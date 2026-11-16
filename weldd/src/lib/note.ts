/** 任务正文:白名单富文本(HTML)+ 255 字纯文本上限 */

export const NOTE_MAX_LEN = 255;

/** 允许保留的内联格式与列表/段落标签,属性一律丢弃(防 style/on* 注入) */
const ALLOWED_TAGS = new Set([
  "B",
  "STRONG",
  "I",
  "EM",
  "U",
  "S",
  "STRIKE",
  "DEL",
  "BR",
  "DIV",
  "P",
  "UL",
  "OL",
  "LI",
]);

/** 这些标签整棵子树丢弃,其余未知标签只剥壳保留内容 */
const DROP_SUBTREE = new Set(["SCRIPT", "STYLE", "IFRAME", "OBJECT", "SVG", "IMG"]);

function rebuild(node: Node, out: Document): Node {
  const tag = node.nodeName;
  if (DROP_SUBTREE.has(tag)) return out.createComment("");
  if (ALLOWED_TAGS.has(tag)) {
    const el = out.createElement(tag);
    for (const child of node.childNodes) {
      const built = rebuild(child, out);
      if (built.nodeName !== "#comment") el.appendChild(built);
    }
    return el;
  }
  if (tag === "#text") return out.createTextNode(node.nodeValue ?? "");
  // 未知元素:剥掉标签壳,递归保留其子内容
  const frag = out.createDocumentFragment();
  for (const child of node.childNodes) {
    const built = rebuild(child, out);
    if (built.nodeName !== "#comment") frag.appendChild(built);
  }
  return frag;
}

/** 解析 HTML 并按白名单重建,任何来源的字符串渲染前都必须过这道 */
export function sanitizeNoteHtml(html: string): string {
  if (!html) return "";
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const out = document.implementation.createHTMLDocument();
  const frag = out.createDocumentFragment();
  for (const child of parsed.body.childNodes) {
    const built = rebuild(child, out);
    if (built.nodeName !== "#comment") frag.appendChild(built);
  }
  const div = out.createElement("div");
  div.appendChild(frag);
  return div.innerHTML;
}

/** 纯文本字数(含换行),255 上限按这个数 */
export function notePlainText(html: string | null): string {
  if (!html) return "";
  const div = document.createElement("div");
  div.innerHTML = html;
  return div.innerText.replace(/\n+$/, "");
}

export function noteTextLength(html: string | null): number {
  return Array.from(notePlainText(html)).length;
}

/** 空正文(纯空白/空行)统一存空串 */
export function emptyToNull(html: string): string {
  return noteTextLength(html) === 0 ? "" : html;
}
