const http = require('http');
const req = http.get('http://127.0.0.1:8080/api/mindmap', (res) => {
  let data = '';
  res.on('data', d => data += d);
  res.on('end', () => {
    const j = JSON.parse(data);
    const depths = j.nodes.map(n => {
      const clean = n.id.replace(/^\\\\\?\\/, '');
      const parts = clean.split(/[\\/]/);
      return parts.length;
    });
    const max = Math.max(...depths);
    console.log('Max folder depth:', max);
    console.log('Total nodes:', j.nodes.length);
    const deepest = j.nodes.filter(n => {
      const clean = n.id.replace(/^\\\\\?\\/, '');
      const parts = clean.split(/[\\/]/);
      return parts.length >= max;
    }).slice(0, 5);
    console.log('\nDeepest nodes:');
    deepest.forEach(n => console.log(n.id));
  });
});
req.on('error', e => console.log('Error:', e.message));