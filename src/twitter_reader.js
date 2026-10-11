(() => {
  'use strict';
  const clean = value => String(value || '').replace(/\u00a0/g, ' ').replace(/\n{3,}/g, '\n\n').trim();
  const parse = value => {
    try {
      const url = new URL(value, 'https://x.com');
      if (!['https:', 'http:'].includes(url.protocol) || !/^(?:(?:www|m|mobile)\.)?(?:x|twitter)\.com$/.test(url.hostname) || url.username || url.password || url.port) return null;
      const match = url.pathname.match(/^\/(?:i\/web|[A-Za-z0-9_]{1,15})\/status\/([1-9][0-9]{0,19})(?:\/(?:photo|video)\/([1-9][0-9]?))?\/?$/)
        || url.pathname.match(/^\/statuses\/([1-9][0-9]{0,19})\/?$/);
      return match ? {id:match[1], href:`https://x.com/i/web/status/${match[1]}`, path:url.pathname} : null;
    } catch { return null; }
  };
  const photo = value => {
    try {
      const url = new URL(value);
      if (url.protocol !== 'https:' || url.hostname !== 'pbs.twimg.com' || url.username || url.password || url.port || !url.pathname.startsWith('/media/')) return null;
      const format = url.searchParams.get('format');
      if (format && !['jpg','jpeg','png','gif','webp'].includes(format)) return null;
      url.search = ''; url.hash = '';
      if (format) url.searchParams.set('format',format);
      url.searchParams.set('name','orig'); return url.href;
    } catch { return null; }
  };
  const own = (node,root) => node.closest('article,[data-testid="tweet"]') === root && !node.closest('[data-testid="quoteTweet"],blockquote,[data-testid="card.wrapper"]');
  const imageBelongs = (img,root,postId) => {
    const anchor = img.closest('a[href]');
    if (anchor) return parse(anchor.getAttribute('href'))?.id === postId;
    // Current public SSR puts the media permalink beside an inert image, not around it.
    // Stop at the first media group so a quoted image cannot borrow the parent post's link.
    let group = img.parentElement;
    for (let depth=0; group && group !== root && depth<5; depth++,group=group.parentElement) {
      const links = [...group.querySelectorAll('a[href]')].map(link=>parse(link.getAttribute('href'))).filter(link=>link && /\/photo\/\d+/.test(link.path));
      if (links.length) return links.some(link=>link.id===postId);
      if (group.matches('[data-testid="tweetPhoto"]')) return true;
    }
    return false;
  };
  const text = node => clean(node?.innerText || node?.textContent || '');
  function extract(doc, currentURL) {
    const target = parse(currentURL);
    if (!target) throw new Error('请打开具体的 X 帖子链接。');
    const candidates = [...doc.querySelectorAll('article,[data-testid="tweet"]')];
    const root = candidates.find(candidate => [...candidate.querySelectorAll('a[href]')].some(anchor => own(anchor,candidate) && parse(anchor.getAttribute('href'))?.id === target.id));
    if (!root) throw new Error('尚未找到所选原帖。请等待加载，遇到登录或验证提示时先完成操作。');
    const controls = [...root.querySelectorAll('button,[role="button"],a')].filter(node=>own(node,root));
    if (controls.some(node => /^(?:show more|read more|显示更多|查看更多|展开|展开全文)$/i.test(text(node)))) {
      throw new Error('原帖还有展开全文按钮，请先展开再获取。');
    }
    if (controls.some(node=>/^(?:订阅以|登录以|Subscribe to|Sign in to)/i.test(text(node)))) throw new Error('原帖需要登录或订阅，请在取得可见正文后获取。');
    const bodyNode = [...root.querySelectorAll('[data-testid="tweetText"]')].find(node=>own(node,root))
      || [...root.querySelectorAll('div[dir="auto"].whitespace-pre-wrap')].find(node=>own(node,root));
    const body = text(bodyNode);
    if (bodyNode) {
      const style = doc.defaultView?.getComputedStyle(bodyNode);
      if (bodyNode.scrollHeight > bodyNode.clientHeight + 8 && ['hidden','clip'].includes(style?.overflowY)) throw new Error('原帖文字仍被折叠，请先展开全文。');
    }
    const links = [...root.querySelectorAll('a[href]')].filter(node=>own(node,root));
    const permalink = links.find(node=>parse(node.getAttribute('href'))?.id===target.id && node.querySelector('time'))
      || links.find(node=>parse(node.getAttribute('href'))?.id===target.id && node.hasAttribute('data-base-ui-tooltip-trigger'));
    const published_at = permalink?.querySelector('time')?.getAttribute('datetime') || text(permalink);
    const handle = permalink?.getAttribute('href')?.match(/^\/(\w+)\/status\//)?.[1];
    const authorRoot = [...root.querySelectorAll('[data-testid="User-Name"]')].find(node=>own(node,root));
    const author = text(authorRoot) || (handle && text(links.find(node=>node.getAttribute('href')===`/${handle}` && text(node) && !text(node).startsWith('@')))) || handle || '';
    const images = [];
    for (const img of root.querySelectorAll('img')) {
      if (!own(img,root)) continue;
      const url = photo(img.getAttribute('src') || img.currentSrc);
      if (!url) continue; // Excludes avatars and video thumbnails.
      if (!imageBelongs(img,root,target.id)) continue;
      if (!images.includes(url)) images.push(url);
    }
    const videoLinks = new Set(links.map(node=>parse(node.getAttribute('href'))).filter(item=>item?.id===target.id && /\/video\/\d+/.test(item.path)).map(item=>item.path));
    const videoElements = [...root.querySelectorAll('video')].filter(node=>own(node,root));
    const videoPlayers = [...root.querySelectorAll('[data-testid="videoPlayer"]')].filter(node=>own(node,root));
    const video_count = Math.max(videoLinks.size,videoElements.length,videoPlayers.length);
    if (!body && !images.length && !video_count) throw new Error('原帖尚未显示可读取的文字或媒体。');
    if (images.length > 50 || video_count > 16) throw new Error('媒体数量异常，请确认页面只打开一条原帖。');
    return {kind:'twitter-post',source:target.href,body,author,published_at,images,video_count,complete:true};
  }
  if (typeof module !== 'undefined' && module.exports) { module.exports={extract,parse,photo}; return; }
  if (window.top !== window) return;
  const install=()=>{
    if (!document.body || !/^(?:(?:www|m|mobile)\.)?(?:x|twitter)\.com$/.test(location.hostname) || document.getElementById('shiwen-twitter-reader')) return;
    const host=document.createElement('div'); host.id='shiwen-twitter-reader'; host.style.cssText='position:fixed;bottom:20px;right:20px;z-index:2147483647';
    const shadow=host.attachShadow({mode:'closed'});
    shadow.innerHTML='<style>:host{all:initial}section{font:14px system-ui,sans-serif;background:#fff;color:#202124;padding:16px;border:1px solid #ddd;border-radius:14px;box-shadow:0 4px 24px #0002;max-width:300px}button{background:#111;color:#fff;border:0;border-radius:8px;padding:10px 16px;margin-top:10px;cursor:pointer}p{margin:0 0 6px;line-height:1.5}</style><section><p><b>拾文 · X / Twitter</b></p><p id="tip">将原帖展开后获取文字和配图。需要登录时请在此窗口完成，回复与引用帖不合并。</p><button>获取当前原帖</button></section>';
    shadow.querySelector('button').onclick=()=>{
      const tip=shadow.getElementById('tip');
      try {
        const data=extract(document,location.href),message=JSON.stringify(data);
        if(new TextEncoder().encode(message).byteLength>4000000)throw new Error('帖子数据过大，本次未保存。');
        if(!window.ipc?.postMessage)throw new Error('请从拾文打开读取窗口。');
        tip.textContent=`已获取 ${data.images.length} 张配图，正在返回拾文…`;window.ipc.postMessage(message);
      }catch(error){tip.textContent=error.message;}
    };
    document.body.appendChild(host);
  };
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',install);else install();
  new MutationObserver(install).observe(document.documentElement||document,{childList:true,subtree:true});
})();
