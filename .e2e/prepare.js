// Complete une archive d'export reelle avec les chemins a tester.
// Chemins construits sans antislash litteral (evite tout probleme d'echappement).
const fs = require('fs');
const crypto = require('crypto');
const B = String.fromCharCode(92);
const p = (...parts) => B + parts.join(B);
const dir = '.e2e/run/export-source';
const man = JSON.parse(fs.readFileSync(dir + '/manifest.json', 'utf8'));
if (!man.tasks.length) { throw new Error('export source vide'); }
const base = man.tasks[0];
const baseXml = fs.readFileSync(dir + '/' + base.xml_file);

function entree(taskPath, fichier) {
  fs.writeFileSync(dir + '/' + fichier, baseXml);
  return Object.assign({}, base, {
    path: taskPath,
    xml_file: fichier,
    sha256: crypto.createHash('sha256').update(baseXml).digest('hex'),
  });
}

const a = entree(p('Microsoft', 'ZTestMsr'), 'Microsoft__ZTestMsr.xml');
const b = entree(p('Microsoft', 'Windows', 'ZTestWin'), 'Microsoft__Windows__ZTestWin.xml');
const c = entree(p('ZTestCollision', 'ZTestMsr'), 'ZTestCollision__ZTestMsr.xml');
fs.unlinkSync(dir + '/' + base.xml_file);
man.tasks = [a, b, c];
fs.writeFileSync(dir + '/manifest.json', JSON.stringify(man, null, 2));
fs.writeFileSync('.e2e/run/answers.json', JSON.stringify(
  { user_map: base.user_id ? { [base.user_id]: base.user_id } : {} }, null, 2));
console.log('entrees : ' + man.tasks.map((t) => t.path).join(' | '));
console.log('utilisateur source : ' + base.user_id + ' (logon ' + base.logon_type + ')');
