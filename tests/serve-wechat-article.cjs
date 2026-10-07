// Dependency-free fixture server. Never serves user files, cookies, or repository secrets.
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
http.createServer((req, res) => {
  const file = req.url === '/src/wechat_article_reader.js' ? 'src/wechat_article_reader.js'
    : ['/', '/tests/wechat_article_reader.html'].includes(req.url) ? 'tests/wechat_article_reader.html' : null;
  if (!file) { res.writeHead(404); return res.end(); }
  res.setHeader('Content-Type', file.endsWith('.js') ? 'text/javascript; charset=utf-8' : 'text/html; charset=utf-8');
  res.end(fs.readFileSync(path.join(root, file)));
}).listen(18765, '127.0.0.1', () => process.stdout.write('http://127.0.0.1:18765/\n'));
