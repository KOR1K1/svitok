// Регенерация core/src/psl_data.rs из Public Suffix List.
// Использование:
//   curl -sSL -o psl.dat https://publicsuffix.org/list/public_suffix_list.dat
//   node scripts/gen_psl.js   (запускать из корня репозитория)
const fs = require('fs');
const toAscii = s => {
  if (/^[\x00-\x7f]*$/.test(s)) return s;
  try { return new URL('http://' + s + '/').hostname; } catch { return null; }
};
const lines = fs.readFileSync('psl.dat','utf8').split('\n');
let version = '';
const rules = new Set(), wild = new Set(), exc = new Set();
for (let raw of lines) {
  const line = raw.trim();
  if (line.startsWith('// VERSION:')) version = line.replace('// VERSION:','').trim();
  if (!line || line.startsWith('//')) continue;
  let kind = 'rule', body = line.toLowerCase();
  if (body.startsWith('!')) { kind = 'exc'; body = body.slice(1); }
  else if (body.startsWith('*.')) { kind = 'wild'; body = body.slice(2); }
  const ascii = toAscii(body);
  if (ascii === null) continue;
  (kind === 'exc' ? exc : kind === 'wild' ? wild : rules).add(ascii);
}
const sort = s => [...s].sort();
const arr = a => a.map(x => JSON.stringify(x)).join(', ');
const out = `//! Public Suffix List, встроенный в бинарник (оффлайн). Сгенерировано из
//! ${version} — НЕ редактировать руками, обновлять регенерацией из
//! https://publicsuffix.org/list/public_suffix_list.dat
//!
//! Только ASCII/punycode-форма правил (host приходит в ней же), три
//! отсортированных набора для бинарного поиска: обычные правила, родители
//! wildcard-правил (\`*.X\` -> X) и исключения (\`!rule\` без \`!\`).

pub static RULES: &[&str] = &[${arr(sort(rules))}];

pub static WILDCARDS: &[&str] = &[${arr(sort(wild))}];

pub static EXCEPTIONS: &[&str] = &[${arr(sort(exc))}];
`;
fs.writeFileSync('core/src/psl_data.rs', out);
console.log(`RULES=${rules.size} WILDCARDS=${wild.size} EXCEPTIONS=${exc.size}, version=${version}`);
