(() => {
  const configurations = {
    bili: {
      items: 'bili-comment-renderer,bili-comment-reply-renderer,.reply-item,.sub-reply-item,.reply-wrap',
      body: '#contents,.reply-content,.reply-text,.text',
      author: '#user-name,.user-name,.user .name,.name',
      time: '#pubdate,.reply-time,.time', likes: '#like,.reply-like,.like',
      count: '#count,.total-reply,.reply-header .total,.reply-navigation .nav-title',
    },
    douyin: {
      items: '[data-e2e="comment-item"],[data-e2e="comment-reply-item"],.comment-item,.comment-reply',
      body: '[data-e2e="comment-content"],.comment-content,.comment-text,.comment-item-info-wrap + div > span',
      author: '[data-e2e="comment-user-name"],[data-e2e="comment-user"],.comment-user-name,.user-name,.comment-item-info-wrap a',
      time: '[data-e2e="comment-time"],.comment-time,.comment-item-info-wrap + div + div', likes: '[data-e2e="comment-like-count"],.like-count,.comment-item-stats-container > div:first-child p:first-child',
      count: '[data-e2e="comment-count"],[data-e2e="comment-list-title"],.comment-title',
    },
    xhs: {
      items: '.comment-item,.parent-comment,.sub-comment',
      body: '.content,.comment-content,.note-text',
      author: '.author .name,.user-name,.username,.name',
      time: '.date,.time', likes: '.like .count,.like-count',
      count: '.comments-container .total,.comments-el .total,.comment-count',
    },
    zhihu: {
      items: '.CommentItemV2,.CommentItem,.CommentItemV2-metaSibling',
      body: '.CommentContent,.CommentItemV2-content,.RichText',
      author: '.UserLink-link,.CommentItemV2-meta .UserLink,.CommentItem-meta .UserLink',
      time: '.CommentItemV2-time,.CommentItem-time', likes: '.CommentItemV2-likeButton,.CommentItem-likeButton',
      count: '.CommentTopbar-title,.CommentTopbar,.CommentsV2-title',
    },
    youtube: {
      items: 'ytd-comment-view-model,ytd-comment-renderer',
      body: '#content-text', author: '#author-text',
      time: '#published-time-text', likes: '#vote-count-middle',
      count: 'ytd-comments-header-renderer #count',
    },
  };
  function platform(host) {
    for (const [name, domains] of Object.entries({bili:['bilibili.com'], douyin:['douyin.com'], xhs:['xiaohongshu.com'], zhihu:['zhihu.com'], youtube:['youtube.com']})) {
      if (domains.some(d => host === d || host.endsWith('.' + d))) return name;
    }
    return null;
  }
  function all(selector, root) {
    const found = [...root.querySelectorAll(selector)];
    if (root.shadowRoot) found.push(...all(selector, root.shadowRoot));
    for (const node of root.querySelectorAll('*')) if (node.shadowRoot) found.push(...all(selector, node.shadowRoot));
    return [...new Set(found)];
  }
  function visible(node) { return node.getClientRects().length > 0; }
  function text(node) { return (node?.innerText || node?.textContent || '').trim(); }
  function bodyText(node) {
    if (!node?.querySelector('img[alt]')) return text(node);
    const read = n => {
      if (n.nodeType === 3) return n.textContent;
      if (n.tagName === 'IMG') return n.getAttribute('alt') || '';
      if (n.tagName === 'BR') return '\n';
      if (['SCRIPT','STYLE'].includes(n.tagName)) return '';
      return [...n.childNodes].map(read).join('') + (['DIV','P'].includes(n.tagName) ? '\n' : '');
    };
    return read(node).trim();
  }
  function first(selector, node) { return all(selector, node).find(visible); }
  function closestDeep(node, selector) {
    while (node) {
      const found = node.closest?.(selector);
      if (found) return found;
      node = node.getRootNode?.().host;
    }
    return null;
  }
  function own(selector, node, itemSelector) {
    return all(selector, node).find(child => {
      const owner = child.closest(itemSelector);
      return visible(child) && (!owner || owner === node);
    });
  }
  function parseTime(value, now = Date.now()) {
    const label = value.trim();
    const absolute = label.match(/(\d{4})[-/年](\d{1,2})[-/月](\d{1,2})日?(?:\s+(\d{1,2}):(\d{2})(?::(\d{2}))?)?/);
    if (absolute) {
      const [,y,m,d,h='0',minute='0',second='0'] = absolute;
      const date = new Date(+y,+m-1,+d,+h,+minute,+second);
      return date.getFullYear() === +y && date.getMonth() === +m-1 && date.getDate() === +d ? date.getTime() : null;
    }
    const relative = label.match(/(\d+)\s*(秒钟?|分钟?|小时|天|周|个月|月|年)前/)
      || label.match(/(\d+)\s*(seconds?|minutes?|hours?|days?|weeks?|months?|years?)\s+ago/i);
    if (relative) {
      const unit = relative[2].toLowerCase();
      let seconds = 1;
      if (/^分|^minute/.test(unit)) seconds = 60;
      else if (/^小时|^hour/.test(unit)) seconds = 3600;
      else if (/^天|^day/.test(unit)) seconds = 86400;
      else if (/^周|^week/.test(unit)) seconds = 604800;
      else if (/^(个月|月)|^month/.test(unit)) seconds = 2592000;
      else if (/^年|^year/.test(unit)) seconds = 31536000;
      return now - Number(relative[1]) * seconds * 1000;
    }
    if (/^(刚刚|just now)/i.test(label)) return now;
    const yesterday = label.match(/(昨天|前天|今天)(?:\s+(\d{1,2}):(\d{2}))?/);
    if (yesterday) {
      const date = new Date(now), days = yesterday[1] === '昨天' ? 1 : yesterday[1] === '前天' ? 2 : 0;
      date.setDate(date.getDate() - days); date.setHours(+(yesterday[2] || 0), +(yesterday[3] || 0), 0, 0);
      return date.getTime();
    }
    const short = label.match(/^(\d{1,2})[-/](\d{1,2})(?:\s+(\d{1,2}):(\d{2}))?/);
    if (short) {
      const date = new Date(new Date(now).getFullYear(), +short[1]-1, +short[2], +(short[3]||0), +(short[4]||0));
      if (date > now) date.setFullYear(date.getFullYear()-1);
      if (date.getMonth() === +short[1]-1 && date.getDate() === +short[2]) return date.getTime();
    }
    return null;
  }
  function parseLikes(value) {
    const match = value.replaceAll(',', '').match(/(\d+(?:\.\d+)?)\s*([万亿kKmM]?)/);
    if (!match) return null;
    const scale = {'万':10000,'亿':100000000,k:1000,m:1000000}[match[2].toLowerCase()] || 1;
    return Math.round(Number(match[1]) * scale);
  }
  function extract(doc, location) {
    const site = platform(location.hostname);
    if (!site) throw new Error('请在支持平台的官方内容页面检测评论');
    const config = configurations[site];
    const comments = [], seen = new Set();
    let limited = false;
    for (const item of all(config.items, doc).filter(visible)) {
      const bodyNode = own(config.body, item, config.items);
      const body = bodyText(bodyNode);
      if (!body) continue;
      const author = text(own(config.author, item, config.items));
      const time = text(own(config.time, item, config.items));
      const likes = text(own(config.likes, item, config.items));
      const replyNode = own('[data-e2e="comment-reply-to"],.reply-to,.replyTo', item, config.items);
      let reply_to = text(replyNode);
      const parent = item.parentElement?.closest(config.items);
      if (!reply_to && parent && parent !== item) reply_to = text(own(config.author, parent, config.items));
      if (!reply_to && item.tagName === 'BILI-COMMENT-REPLY-RENDERER') {
        const thread = closestDeep(item, 'bili-comment-thread-renderer');
        const root = thread && all('bili-comment-renderer', thread)[0];
        reply_to = root ? text(own(config.author, root, config.items)) : '';
      }
      if (!reply_to && item.closest('ytd-comment-replies-renderer')) {
        const thread = item.closest('ytd-comment-thread-renderer');
        reply_to = text(thread?.querySelector('#author-text'));
      }
      const answer = item.closest('.AnswerItem,.List-item');
      const thread = site === 'zhihu' && answer ? text(answer.querySelector('.AuthorInfo-name,.UserLink-link')) : '';
      const domId = item.getAttribute('id') || '';
      const tooltipId = site === 'douyin' ? own('[id^="tooltip_"]', item, config.items)?.id?.replace('tooltip_', '') : '';
      const id = item.getAttribute('data-comment-id') || item.getAttribute('data-id') || item.getAttribute('data-rpid') || tooltipId || (/[0-9]/.test(domId) ? domId : '');
      const key = id || `${author}\n${body}\n${reply_to}\n${thread}`;
      if (seen.has(key)) continue;
      seen.add(key);
      if (body.length > 20000) throw new Error('有评论过长，请分批获取，避免截断正文');
      if (comments.length >= 1000) { limited = true; break; }
      comments.push({id,author,body,time,likes,reply_to,thread,like_count:parseLikes(likes),posted_at:parseTime(time)});
    }
    let total = text(first(config.count, doc)).slice(0, 200);
    if (site === 'douyin' && !/\d/.test(total)) {
      total = text(all('main *', doc).find(n=>visible(n) && /^评论[（(]\s*[\d,.万亿]+\s*[)）]$/.test(text(n))));
    }
    return {source:location.href, title:doc.title, comments, total_label:total, limited};
  }
  if (typeof module !== 'undefined' && module.exports) module.exports = {extract, platform, parseTime, parseLikes};
  if (typeof window === 'undefined' || window.top !== window) return;
  let cache = new Map(), source = '';
  function install() {
    if (!document.body || !platform(location.hostname) || document.getElementById('shiwen-comments')) return;
    const host = document.createElement('div');
    host.id = 'shiwen-comments';
    host.style.cssText = 'position:fixed;bottom:18px;right:18px;z-index:2147483647';
    const shadow = host.attachShadow({mode:'closed'});
    shadow.innerHTML = `<style>:host{all:initial}section{font:14px sans-serif;background:white;color:#222;padding:14px;border:1px solid #ddd;border-radius:12px;box-shadow:0 4px 22px #0002;max-width:310px}p{margin:0 0 8px;line-height:1.5}button{border:1px solid #ccc;border-radius:8px;background:#f5f5f5;color:#222;padding:9px;margin:4px 5px 0 0;cursor:pointer}button:last-child{background:#222;color:white}</style><section><p><b>拾文 · 评论</b></p><p>打开评论区、展开所需回复后检测。可继续滚动加载，再检测追加。</p><p id="tip">尚未检测，不会自动导出。</p><button id="detect">检测已加载评论</button><button id="return">返回并选择评论</button></section>`;
    const detect = () => {
      try {
        const data = extract(document, location);
        if (source && source !== location.href) cache.clear();
        source = location.href;
        for (const item of data.comments) {
          const key = item.id || `${item.author}\n${item.body}\n${item.reply_to}\n${item.thread}`;
          if (cache.size < 1000 || cache.has(key)) cache.set(key, item);
        }
        data.comments = [...cache.values()];
        shadow.getElementById('tip').textContent = data.comments.length ? `已检测 ${data.comments.length} 条（含已展开回复）。${data.total_label ? '页面数量：' + data.total_label + '。' : '页面未显示总数。'}${data.limited || cache.size >= 1000 ? '本次达到 1000 条上限。' : ''}仅包含已加载评论。` : '未读取到评论，请打开评论区或展开回复后重试；不是评论数为零。';
        return data;
      } catch (error) { shadow.getElementById('tip').textContent = error.message; return null; }
    };
    shadow.getElementById('detect').onclick = detect;
    shadow.getElementById('return').onclick = () => {
      const data = detect();
      if (data?.comments.length) window.ipc.postMessage(JSON.stringify(data));
    };
    document.body.appendChild(host);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', install); else install();
  new MutationObserver(install).observe(document.documentElement || document, {childList:true,subtree:true});
})();
