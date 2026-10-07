(() => {
  'use strict';
  const clean = value => String(value || '').replace(/\u00a0/g, ' ').replace(/[ \t]+\n/g, '\n').replace(/\n[ \t]+/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
  const articleURL = input => {
    try {
      const url = new URL(input);
      if (!['https:', 'http:'].includes(url.protocol) || url.hostname !== 'mp.weixin.qq.com' || url.username || url.password || url.port) return null;
      if (!/^\/s\/[\w-]+$/.test(url.pathname) && !(url.pathname === '/s' && ['__biz', 'mid', 'idx', 'sn'].every(key => url.searchParams.get(key)))) return null;
      url.protocol = 'https:'; url.hash = ''; return url.href;
    } catch { return null; }
  };
  const imageURL = input => {
    try {
      const url = new URL(input, 'https://mp.weixin.qq.com/');
      if (url.protocol === 'http:') url.protocol = 'https:';
      return url.protocol === 'https:' && !url.username && !url.password && !url.port &&
        (url.hostname === 'qpic.cn' || url.hostname.endsWith('.qpic.cn') || url.hostname === 'mmbiz.qlogo.cn') ? url.href : null;
    } catch { return null; }
  };
  const visible = node => {
    if (!node || node.hidden || node.closest('[hidden],[aria-hidden="true"]')) return false;
    const win = node.ownerDocument.defaultView;
    for (let item = node; item && item.nodeType === 1; item = item.parentElement) {
      const style = win?.getComputedStyle(item);
      if (style?.display === 'none' || style?.visibility === 'hidden') return false;
    }
    return true;
  };
  const text = node => clean(node?.innerText || node?.textContent || '');
  function extract(doc, sourceURL) {
    const source = articleURL(sourceURL);
    if (!source) throw new Error('请在微信公众号的官方文章页面获取正文。');
    const root = doc.querySelector('#js_content');
    const title = text(doc.querySelector('#activity-name')) || clean(doc.querySelector('meta[property="og:title"]')?.content);
    const errorPattern = /该内容已被发布者删除|此内容因违规无法查看|内容已被删除|该内容已被投诉|环境异常|访问过于频繁|请完成验证|安全验证|访问受限|此内容无法查看/;
    for (const node of doc.querySelectorAll('.weui-msg,#js_verify,#verify_container,.verify_container,[role="alert"]')) {
      if (visible(node) && errorPattern.test(text(node))) throw new Error('该文章暂时无法读取；请先在窗口中完成验证，或确认文章仍可访问。');
    }
    if (!root || !visible(root) || !title) throw new Error('未找到完整文章。请等待加载，遇到微信验证或登录提示时先在此窗口完成操作。');
    // Only inspect known access controls, not prose discussing login or paid articles.
    const accessPattern = /付费阅读|付费后|购买后|试看|剩余.*(?:付费|购买)|登录后.*(?:阅读|查看)|微信.*打开.*(?:全文|阅读)|展开全文|阅读全文/;
    for (const node of doc.querySelectorAll('#js_pay,#js_pay_area,#js_pay_preview,.paywall,.pay_reading,.pay_content,[class*="paywall"],#js_readmore_area,#js_readmore_btn,#js_login,.login_dialog,.weui-dialog')) {
      if (visible(node) && accessPattern.test(text(node))) throw new Error('页面仍有付费、登录或展开全文提示，当前只是预览；请先取得完整正文再获取。');
    }
    const style = doc.defaultView?.getComputedStyle(root);
    if (root.scrollHeight > root.clientHeight + 8 && ['hidden', 'clip'].includes(style?.overflowY)) {
      throw new Error('正文仍被折叠，请先展开全文再获取。');
    }
    const images = [];
    const walk = node => {
      if (node.nodeType === 3) return node.nodeValue.replace(/\s+/g, ' ');
      if (node.nodeType !== 1 || !visible(node)) return '';
      const tag = node.tagName.toLowerCase();
      if (['script', 'style', 'noscript', 'iframe', 'button', 'input'].includes(tag)) return '';
      if (tag === 'br') return '\n';
      if (tag === 'img') {
        const raw = node.getAttribute('data-src') || node.currentSrc || node.getAttribute('src') || '';
        // Ignore transparent layout spacers, but preserve every real article image.
        if (!raw || raw.startsWith('data:')) return '';
        const url = imageURL(raw);
        if (!url) throw new Error('正文包含暂不支持的图片来源，未将其忽略。请确认文章配图加载正常后重试。');
        if (!images.includes(url)) images.push(url);
        return `\n\n![配图${images.indexOf(url) + 1}](${url})\n\n`;
      }
      let content = [...node.childNodes].map(walk).join('');
      if (/^h[1-6]$/.test(tag)) return `\n\n${'#'.repeat(Number(tag[1]))} ${clean(content)}\n\n`;
      if (tag === 'li') return `\n- ${clean(content)}\n`;
      if (tag === 'td' || tag === 'th') return content + '\t';
      if (['p', 'div', 'section', 'article', 'blockquote', 'ul', 'ol', 'tr', 'table'].includes(tag)) content = '\n\n' + content + '\n\n';
      return content;
    };
    const body = clean(walk(root));
    if (!body && !images.length) throw new Error('文章正文为空，请等待加载完成后重试。');
    if (images.length > 300) throw new Error('文章配图超过 300 张，本次未保存。');
    if ([...title].length > 500) throw new Error('文章标题过长，请确认打开的是正文页面。');
    return {
      kind: 'wechat-article', source, title, body, images, complete: true, incomplete: false, selected: false,
      author: text(doc.querySelector('#js_name')) || text(doc.querySelector('#profileBt')),
      published_at: text(doc.querySelector('#publish_time')),
    };
  }
  if (typeof module !== 'undefined' && module.exports) { module.exports = {extract, articleURL, imageURL}; return; }
  if (window.top !== window) return;
  const install = () => {
    if (!document.body || location.hostname !== 'mp.weixin.qq.com' || document.getElementById('shiwen-wechat-reader')) return;
    const host = document.createElement('div');
    host.id = 'shiwen-wechat-reader';
    host.style.cssText = 'position:fixed;bottom:20px;right:20px;z-index:2147483647';
    const shadow = host.attachShadow({mode:'closed'});
    shadow.innerHTML = `<style>:host{all:initial}section{font:14px system-ui,sans-serif;background:#fff;color:#202124;padding:16px;border:1px solid #ddd;border-radius:14px;box-shadow:0 4px 24px #0002;max-width:300px}button{background:#07a55c;color:white;border:0;border-radius:8px;padding:10px 16px;margin-top:10px;cursor:pointer}p{margin:0 0 6px;line-height:1.5}</style><section><p><b>拾文 · 微信公众号</b></p><p id="tip">文章显示完整后获取正文和配图，回到拾文预览并确认导出。</p><button>获取文章正文</button></section>`;
    shadow.querySelector('button').onclick = () => {
      const tip = shadow.getElementById('tip');
      try {
        const data = extract(document, location.href);
        if (!window.ipc?.postMessage) throw new Error('请从拾文打开文章窗口。');
        const message = JSON.stringify(data);
        if (new TextEncoder().encode(message).byteLength > 4000000) throw new Error('文章数据超过可处理大小，本次未保存。');
        tip.textContent = `已读取正文与 ${data.images.length} 张配图，正在返回拾文…`;
        window.ipc.postMessage(message);
      } catch (error) { tip.textContent = error.message; }
    };
    document.body.appendChild(host);
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', install);
  else install();
  new MutationObserver(install).observe(document.documentElement || document, {childList:true, subtree:true});
})();
