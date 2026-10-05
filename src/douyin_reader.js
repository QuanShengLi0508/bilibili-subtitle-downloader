(() => {
  if (window.top !== window) return;
  const allowed = () => /(^|\.)(douyin|iesdouyin)\.com$/.test(location.hostname);
  const install = () => {
    if (!document.body || !allowed() || document.getElementById('shiwen-reader')) return;
    const host = document.createElement('div');
    host.id = 'shiwen-reader';
    host.style.cssText = 'position:fixed;bottom:20px;right:20px;z-index:2147483647';
    const shadow = host.attachShadow({mode:'closed'});
    shadow.innerHTML = `<style>:host{all:initial}div{font:14px sans-serif;background:#fff;color:#15294c;padding:14px;border:1px solid #d9e6fc;border-radius:12px;box-shadow:0 4px 24px #0003;max-width:320px}button{background:#2479ff;color:#fff;border:0;border-radius:8px;padding:10px;margin:8px 4px 0 0;cursor:pointer}p{margin:0 0 5px;line-height:1.5}</style><div><p>拾文 · 长文章</p><p id="tip">请先阅读全文。获取后回到拾文预览，再确认导出。</p><button id="article">获取文章正文</button><button id="selection">获取选中文字</button></div>`;
    const read = (selected) => {
      const tip = shadow.getElementById('tip');
      let body = '', title = document.title.replace(/\s*[-–|]\s*抖音.*$/, '');
      let root;
      if (selected) body = window.getSelection()?.toString() || '';
      else {
        const selectors = 'article,[data-e2e="article-content"],.article-content,[class*="articleContent"],[class*="article-content"]';
        root = [...document.querySelectorAll(selectors)].filter(node => node.getClientRects().length)
          .sort((a,b) => (b.innerText || '').length - (a.innerText || '').length)[0];
        if (!root) { tip.textContent = '未识别到文字正文。可选中正文后获取；图片文章暂不支持识别。'; return; }
        body = root.innerText || '';
        title = root.querySelector('h1')?.innerText || title;
      }
      if (body.trim().length < 20) { tip.textContent = '请展开文章或选中需要导出的正文后重试。'; return; }
      const incomplete = !selected && /阅读全文|展开全文|登录后.*(阅读|查看)/.test(root.innerText || '');
      if (incomplete) { tip.textContent = '页面还有展开或登录提示，请先取得正文再获取。'; return; }
      window.ipc.postMessage(JSON.stringify({title,body,source:location.href,selected,incomplete}));
    };
    shadow.getElementById('article').onclick = () => read(false);
    shadow.getElementById('selection').onclick = () => read(true);
    for (const button of shadow.querySelectorAll('button')) button.onmousedown = event => event.preventDefault();
    document.body.appendChild(host);
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded',install);
  else install();
  new MutationObserver(install).observe(document.documentElement || document,{childList:true,subtree:true});
})();
