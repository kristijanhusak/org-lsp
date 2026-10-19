// Entry point VS Code loads in its extension host: runs the tests with Mocha,
// those of `ORG_LSP_TEST_SUITE` (`extension`, `highlight`, `scenarios`).
const path = require('node:path');
const Mocha = require('mocha');

exports.run = function run() {
  const mocha = new Mocha({ ui: 'bdd', color: true, timeout: 30000 });
  mocha.addFile(path.join(__dirname, `${process.env.ORG_LSP_TEST_SUITE || 'extension'}.test.cjs`));
  return new Promise((resolve, reject) => {
    mocha.run((failures) => (failures > 0 ? reject(new Error(`${failures} test(s) failed`)) : resolve()));
  });
};
