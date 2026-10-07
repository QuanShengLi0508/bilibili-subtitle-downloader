const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const script = fs.readFileSync(path.join(__dirname, '../src/wechat_channels_reader.js'), 'utf8');

function read(videos, hostname = 'channels.weixin.qq.com') {
  const sent = [];
  const nodes = [];
  const document = {
    readyState: 'complete', title: '公开作品 - 视频号',
    body: {append(node) { nodes.push(node); }},
    getElementById(id) { return nodes.find(node => node.id === id); },
    querySelectorAll(selector) { return selector === 'video' ? videos : []; },
    querySelector() { return null; },
    createElement(tag) {
      return {tag, style: {}, children: [], events: {},
        addEventListener(name, handler) { this.events[name] = handler; },
        append(...children) { this.children.push(...children); }};
    }
  };
  const window = {ipc: {postMessage(message) { sent.push(JSON.parse(message)); }}};
  window.top = window;
  vm.runInNewContext(script, {window, document, location: {hostname, href: `https://${hostname}/web/pages/feed?oid=public-test`}, innerHeight: 800, URL, setInterval() {}, addEventListener() {}});
  const bar = nodes[0];
  if (!bar) return {sent, absent: true};
  bar.children[1].events.click();
  return {sent, text: bar.children[0].textContent};
}
const video = (src, extra = {}) => ({currentSrc: src, src, readyState: 4, duration: 20, paused: false, ended: false,
  getBoundingClientRect: () => ({width: 320, height: 180, top: 100, bottom: 280}), ...extra});
const valid = 'https://finder.video.qq.com/public.mp4?token=preserved';
const result = read([video(valid)]);
assert.equal(result.sent.length, 1);
assert.equal(result.sent[0].media_url, valid);
assert.equal(result.sent[0].title, '公开作品');
assert.equal(read([video(valid)], 'evil.test').absent, true);
assert.equal(read([video('blob:https://channels.weixin.qq.com/test')]).sent.length, 0);
assert.match(read([video('blob:https://channels.weixin.qq.com/test')]).text, /临时播放流/);
assert.equal(read([video('https://finder.video.qq.com.evil.test/a.mp4')]).sent.length, 0);
assert.equal(read([video(valid, {readyState: 0})]).sent.length, 0);
assert.equal(read([video(valid, {duration: Infinity})]).sent.length, 0);
assert.equal(read([video(valid), video(valid)]).sent.length, 0);
assert.equal(read([video(valid), video('https://finder.video.qq.com/other.mp4', {paused:true})]).sent.length, 1);
assert.equal(read([video(valid, {getBoundingClientRect: () => ({width:0,height:0,top:0,bottom:0})})]).sent.length, 0);
console.log('Video Channels reader: 10 fixture checks passed.');
