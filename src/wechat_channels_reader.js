(() => {
  if (window.top !== window || !['channels.weixin.qq.com', 'weixin.qq.com'].includes(location.hostname)) return;
  let sent = false;
  function source() {
    const visible = [...document.querySelectorAll('video')].filter(video => {
      const box = video.getBoundingClientRect();
      return box.width > 0 && box.height > 0 && box.bottom > 0 && box.top < innerHeight;
    });
    const playing = visible.filter(video => !video.paused && !video.ended);
    if (playing.length === 1) return playing[0];
    return visible.length === 1 ? visible[0] : null;
  }
  function inspect() {
    const video = source();
    if (!video) return {error:'请先打开并播放要保存的作品。仅在微信内可看的视频卡片，暂时不能从这里读取。'};
    if (video.readyState < 2 || !Number.isFinite(video.duration) || video.duration <= 0) {
      return {error:'视频尚未正常播放。请按官方页面提示登录或完成加载，再点击读取。'};
    }
    const address = video.currentSrc || video.src || video.querySelector('source[src]')?.src;
    let url;
    try { url = new URL(address); } catch (_) { return {error:'当前视频没有可读取的普通播放地址。'}; }
    if (url.protocol === 'blob:') return {error:'当前作品使用临时播放流，暂不支持直接下载。可导入已有本地视频进行转写。'};
    if (url.protocol !== 'https:' || !['finder.video.qq.com', 'wxapp.tc.qq.com'].includes(url.hostname) || url.username || url.password || url.port) {
      return {error:'当前作品未提供支持的官方视频直链。暂不支持此播放方式。'};
    }
    const title = document.querySelector('meta[property="og:title"]')?.content || document.title || '视频号视频';
    return {source:location.href, title:title.replace(/\s*[-|·]\s*(微信)?视频号\s*$/, '').trim() || '视频号视频', media_url:url.href};
  }
  function mount() {
    if (!document.body || document.getElementById('shiwen-channels-reader')) return;
    const bar = document.createElement('aside');
    bar.id = 'shiwen-channels-reader';
    bar.style.cssText = 'position:fixed;bottom:16px;left:16px;right:16px;z-index:2147483647;display:flex;gap:16px;align-items:center;padding:14px 18px;border:1px solid #ddd;border-radius:14px;background:#fff;color:#222;font:14px/1.5 system-ui,sans-serif;box-shadow:0 4px 25px #0002';
    const text = document.createElement('span');
    text.style.cssText = 'flex:1;min-width:0;white-space:normal';
    text.textContent = '在官方页面正常播放视频后点击读取；需要登录时请按页面提示操作。读取成功后会检查视频是否可播放。';
    const button = document.createElement('button');
    button.textContent = '读取当前视频';
    button.style.cssText = 'flex:none;padding:10px 16px;border:0;border-radius:10px;background:#202020;color:white;font:inherit;cursor:pointer';
    button.addEventListener('click', () => {
      if (sent) return;
      const result = inspect();
      if (result.error) { text.textContent = result.error; return; }
      if (!window.ipc?.postMessage) { text.textContent = '请在拾文的视频号窗口中读取。'; return; }
      sent = true;
      button.disabled = true;
      text.textContent = '已读取播放地址，正在返回拾文。下载后将验证视频完整性。';
      window.ipc.postMessage(JSON.stringify(result));
    });
    bar.append(text, button);
    document.body.append(bar);
  }
  if (document.readyState === 'loading') addEventListener('DOMContentLoaded', mount, {once:true});
  else mount();
  setInterval(mount, 1500);
})();
