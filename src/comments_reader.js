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
    if (['x.com','www.x.com','mobile.x.com','twitter.com','www.twitter.com','mobile.twitter.com'].includes(host)) return 'twitter';
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
    if (!node?.querySelector('img[alt],img.note-content-emoji')) return text(node);
    const read = n => {
      if (n.nodeType === 3) return n.textContent;
      if (n.tagName === 'IMG') return n.getAttribute('alt') || (n.classList.contains('note-content-emoji') ? '[表情]' : '');
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
  function xStatus(input) {
    try {
      const url = new URL(input, 'https://x.com');
      if (platform(url.hostname) !== 'twitter' || !['https:','http:'].includes(url.protocol) || url.username || url.password || url.port) return null;
      const match = url.pathname.match(/^\/(?:[A-Za-z0-9_]+\/status|i\/web\/status)\/(\d{1,22})(?:\/(?:photo|video)\/\d+)?\/?$/);
      return match?.[1] || null;
    } catch { return null; }
  }
  function xError(message, code = 'not_loaded') { return Object.assign(new Error(message), {code}); }
  function xOwn(node, item) {
    if (node.closest('article') !== item) return false;
    const quote = node.closest('[data-testid="quoteTweet"],[data-testid="card.wrapper"],[role="link"][data-href]');
    return !quote || !item.contains(quote);
  }
  function xPost(item) {
    // Read only a post's own permalink, never a quoted post or a media link.
    const links = [...item.querySelectorAll('a[href]')].filter(node => xOwn(node, item) && !/\/(photo|video)\//.test(node.getAttribute('href')));
    const anchor = links.find(node => node.querySelector('time') && xStatus(node.getAttribute('href')))
      || links.find(node => node.hasAttribute('data-base-ui-tooltip-trigger') && xStatus(node.getAttribute('href')));
    if (!anchor) return null;
    return {id:xStatus(anchor.getAttribute('href')), anchor};
  }
  function extractX(doc, location) {
    const id = xStatus(location.href);
    const main = doc.querySelector('[data-testid="primaryColumn"],main');
    const missing = /(?:post|tweet) (?:is )?unavailable|this (?:post|tweet).*deleted|page (?:doesn.t|does not) exist|该页面不存在|此帖.*(?:不存在|不可用)|帖子.*(?:不存在|已删除|不可用)|这些帖子受保护/i;
    const notices = [...doc.querySelectorAll('[role="alert"],[data-testid="error-detail"],main')].filter(visible);
    if (notices.some(node => !node.querySelector('article') && missing.test(text(node)))) {
      throw xError('X 帖子不存在、已删除或当前账号无权查看；未读取任何回复。', 'unavailable');
    }
    const login = /\/i\/(?:flow\/login|jf\/onboarding)|\/login/.test(location.href)
      || [...doc.querySelectorAll('[role="dialog"]')].filter(visible).some(node => /log in|sign in|登录/i.test(text(node)));
    if (login) throw xError('请先在 X 官方窗口登录，然后回到原帖展开回复并重新检测。', 'login_required');
    if (!id) throw xError('请回到要提取回复的原帖详情页，个人主页和搜索结果不作为评论来源。', 'changed_post');
    if (!main) throw xError('X 原帖尚未显示，请等待加载或在官方页面登录后重试。');
    const posts = [...main.querySelectorAll('article')].filter(node => visible(node) && !node.parentElement.closest('article'));
    const root = posts.find(node => xPost(node)?.id === id);
    if (!root) throw xError('未找到链接中的原帖。请回到原帖并等待加载；不会把推荐帖子当作评论。', 'changed_post');
    const list = root.closest('ul');
    const legacy = root.closest('[role="region"][aria-label]');
    const scope = list && main.contains(list) && root.closest('[data-timeline-entry]') ? list
      : legacy && /conversation|对话|会话/i.test(legacy.getAttribute('aria-label')) ? legacy : null;
    if (!scope) throw xError('当前页面没有可确认的原帖会话列表。请打开帖子详情并展开回复后重试。');
    const after = node => !!(root.compareDocumentPosition(node) & 4);
    const barriers = [...scope.querySelectorAll('h1,h2,h3,[role="heading"],[data-testid="placementTracking"],[data-testid="promotedIndicator"]')]
      .filter(node => visible(node) && !node.closest('article') && after(node));
    const comments = [], seen = new Set();
    let limited = false;
    for (const item of posts) {
      if (!scope.contains(item) || !after(item) || item === root) continue;
      if (list && item.closest('ul') !== list) continue;
      if (barriers.some(barrier => !!(barrier.compareDocumentPosition(item) & 4))) break;
      const row = item.closest('[data-timeline-entry],[data-testid="cellInnerDiv"]') || item;
      if (row.querySelector('[data-testid="placementTracking"],[data-testid="promotedIndicator"]') || /^(promoted|广告)$/im.test(text(row.querySelector('[data-testid="socialContext"]')))) continue;
      const post = xPost(item);
      if (!post?.id || post.id === id || seen.has(post.id)) continue;
      if (list) {
        const linkedRow = item.closest('[data-timeline-entry][data-href]');
        if (!linkedRow || xStatus(linkedRow.getAttribute('data-href')) !== post.id) continue;
      }
      if ([...item.querySelectorAll('[data-testid="tweet-text-show-more-link"],button,a')].some(node => xOwn(node, item) && /^(show more|显示更多|展开全文)$/i.test(text(node)))) {
        throw xError('有 X 会话正文仍被折叠，请先点击“显示更多”展开后再检测，避免截断。');
      }
      const bodyNode = [...item.querySelectorAll('[data-testid="tweetText"],div[dir="auto"].whitespace-pre-wrap')].find(node => xOwn(node, item));
      let body = bodyText(bodyNode);
      const media = [...item.querySelectorAll('a[href],video')].filter(node => xOwn(node, item) && (node.tagName === 'VIDEO' || /\/(photo|video)\/\d/.test(node.getAttribute('href'))));
      if (!body && media.length) body = '[此会话条目仅含图片或视频，无文字正文]';
      if (!body) continue;
      if (body.length > 20000) throw xError('有 X 会话正文过长，请分批读取；不会自动截断。');
      const authorNode = [...item.querySelectorAll('[data-testid="User-Name"] a,a[href]')].find(node => xOwn(node, item) && /^@[A-Za-z0-9_]+$/.test(text(node)));
      const author = text(authorNode);
      const timeNode = post.anchor.querySelector('time');
      const time = text(timeNode || post.anchor);
      const date = timeNode?.getAttribute('datetime');
      const absoluteTime = date ? Date.parse(date) : NaN;
      const likeNode = [...item.querySelectorAll('[data-engagement-action="like"],[data-testid="like"],[data-testid="unlike"]')].find(node => xOwn(node, item));
      const likes = text(likeNode) || likeNode?.getAttribute('aria-label') || '';
      const replyMarker = [...item.querySelectorAll('[data-testid="replyingTo"],div,span')].find(node => xOwn(node, item) && /^(Replying to|回复)\s*@/i.test(text(node)) && text(node).length < 180);
      comments.push({id:post.id, author, body, time, likes, like_count:parseLikes(likes), posted_at:Number.isFinite(absoluteTime) ? absoluteTime : parseTime(time),
        reply_to:text(replyMarker), thread:'当前原帖下已加载会话条目（可能含作者串帖，不保证均为直接回复）', conversation_source_id:id, scope:'loaded-conversation'});
      seen.add(post.id);
      if (comments.length >= 1000) { limited = true; break; }
    }
    return {source:location.href, title:doc.title, comments, limited,
      total_label:'已加载会话条目（可能含作者串帖，不保证均为直接回复）',
      notice: comments.length ? '' : '尚未读取到可确认的 X 会话条目；请登录、展开回复或滚动加载后再检测，这不代表零评论。'};
  }
  function extract(doc, location) {
    const site = platform(location.hostname);
    if (!site) throw new Error('请在支持平台的官方内容页面检测评论');
    if (site === 'twitter') return extractX(doc, location);
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
      if (!reply_to && site === 'xhs' && item.matches('.comment-item-sub,.sub-comment')) {
        const group = item.closest('.parent-comment');
        const root = group && [...group.querySelectorAll('.comment-item')].find(n => !n.matches('.comment-item-sub,.sub-comment'));
        reply_to = root ? text(own(config.author, root, config.items)) : '';
      }
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
  if (typeof module !== 'undefined' && module.exports) { module.exports = {extract, platform, parseTime, parseLikes, xStatus}; return; }
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
        const countLabel = platform(location.hostname) === 'twitter' ? '范围：' : '页面数量：';
        shadow.getElementById('tip').textContent = data.comments.length ? `已检测 ${data.comments.length} 条。${data.total_label ? countLabel + data.total_label + '。' : '页面未显示总数。'}${data.limited || cache.size >= 1000 ? '本次达到 1000 条上限。' : ''}仅包含已加载评论。` : data.notice || '未读取到评论，请打开评论区或展开回复后重试；不是评论数为零。';
        return data;
      } catch (error) { shadow.getElementById('tip').textContent = error.message; return {kind:'comments-error',source:location.href,code:error.code || 'not_loaded'}; }
    };
    shadow.getElementById('detect').onclick = detect;
    shadow.getElementById('return').onclick = () => {
      const data = detect();
      if (!window.ipc?.postMessage) { shadow.getElementById('tip').textContent = '请从拾文的“检测评论”打开此窗口。'; return; }
      if (data?.kind === 'comments-error' || data?.comments) window.ipc.postMessage(JSON.stringify(data));
    };
    document.body.appendChild(host);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', install); else install();
  new MutationObserver(install).observe(document.documentElement || document, {childList:true,subtree:true});
})();
