const http=require('node:http'),fs=require('node:fs'),path=require('node:path');
const root=path.resolve(__dirname,'..');
http.createServer((req,res)=>{
  const file=req.url==='/src/twitter_reader.js'?'src/twitter_reader.js':['/','/tests/twitter_reader.html'].includes(req.url)?'tests/twitter_reader.html':null;
  if(!file){res.writeHead(404);return res.end();}
  res.setHeader('Content-Type',file.endsWith('.js')?'text/javascript; charset=utf-8':'text/html; charset=utf-8');res.end(fs.readFileSync(path.join(root,file)));
}).listen(18766,'127.0.0.1',()=>process.stdout.write('http://127.0.0.1:18766/\n'));
