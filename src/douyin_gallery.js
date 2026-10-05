(() => {
  if (window.top !== window) return;
  let sent = false;
  function collect() {
    if (sent || !/^\/note\/\d+\/?$/.test(location.pathname)) return;
    const main = document.querySelector('main[data-e2e="note-detail"]');
    const slides = main?.querySelector('.focusPanel')?.querySelectorAll('.dySwiperSlide');
    if (!slides?.length) return;
    const images = [];
    for (const slide of slides) {
      const image = slide.querySelector('img');
      const source = image?.currentSrc || image?.getAttribute('src') || image?.getAttribute('data-src');
      if (!source || !image.complete || image.naturalWidth < 100 || !/^https:\/\//.test(source)) return;
      const address = new URL(source);
      if (!/(^|\.)(douyinpic\.com|byteimg\.com)$/.test(address.hostname)) return;
      if (!images.includes(source)) images.push(source);
    }
    const user = main.querySelector('[data-e2e="user-info"]');
    const description = user?.nextElementSibling?.innerText?.split('发布时间：')[0]?.replace(/\s*展开\s*$/, '').trim();
    if (!description) return;
    const message = { source: location.origin + location.pathname, title: description.slice(0, 70),
      description, author: user.querySelector('a img')?.getAttribute('alt') || '', images };
    sent = true;
    window.ipc.postMessage(JSON.stringify(message));
  }
  setInterval(collect, 1200);
  addEventListener('DOMContentLoaded', () => {
    const bar = document.createElement('div');
    bar.style.cssText = 'position:fixed;bottom:12px;left:12px;z-index:2147483647;background:#fff;color:#16386b;padding:12px 18px;border-radius:10px;box-shadow:0 2px 15px #888;font:14px sans-serif;max-width:650px';
    bar.textContent = '拾文正在读取作品配图和文案，完成后自动返回。若有登录提示，请登录；取消请关闭窗口。';
    document.body.append(bar);
    collect();
  });
})();
