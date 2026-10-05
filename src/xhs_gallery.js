(() => {
  if (window.top !== window) return;
  let sent = false;
  function collect() {
    if (sent || !/(^|\.)xiaohongshu\.com$/.test(location.hostname)) return;
    const id = location.pathname.match(/\/(?:explore|discovery\/item)\/([a-f\d]{24})(?:\/|$)/i)?.[1];
    if (!id) return;
    const state = window.__INITIAL_STATE__;
    const note = state?.note?.noteDetailMap?.[id]?.note;
    if (!note || note.type !== 'normal' || !note.imageList?.length) return;
    const images = [];
    for (const entry of note.imageList) {
      const source = entry.urlDefault || entry.infoList?.find(i => i.imageScene === 'WB_DFT')?.url || entry.infoList?.[0]?.url;
      if (!source) return;
      const address = new URL(source, location.href);
      if (!/(^|\.)xhscdn\.com$/.test(address.hostname)) return;
      address.protocol = 'https:';
      images.push(address.href);
    }
    sent = true;
    window.ipc.postMessage(JSON.stringify({ source: location.href, title: note.title || '小红书图文',
      description: note.desc || '', author: note.user?.nickname || '', images }));
  }
  setInterval(collect, 1200);
  addEventListener('DOMContentLoaded', () => {
    const bar = document.createElement('div');
    bar.style.cssText = 'position:fixed;bottom:12px;left:12px;z-index:2147483647;background:#fff;color:#16386b;padding:12px 18px;border-radius:10px;box-shadow:0 2px 15px #888;font:14px sans-serif;max-width:650px';
    const text = document.createElement('span');
    text.textContent = /\/(?:explore|discovery\/item)\/[a-f\d]{24}/i.test(location.pathname)
      ? '拾文正在读取小红书图文，完成后自动返回。若要求登录，请手动登录；取消请关闭窗口。'
      : '请在小红书官方页面登录，完成后关闭窗口，再回到拾文粘贴笔记分享文字。';
    bar.append(text);
    document.body.append(bar);
    setInterval(() => {
      const id = location.pathname.match(/\/(?:explore|discovery\/item)\/([a-f\d]{24})(?:\/|$)/i)?.[1];
      const note = window.__INITIAL_STATE__?.note?.noteDetailMap?.[id]?.note;
      if (note?.type === 'video') text.textContent = '这条是小红书视频笔记，请关闭窗口，切换到「下载视频」获取。';
    }, 1500);
    collect();
  });
})();
