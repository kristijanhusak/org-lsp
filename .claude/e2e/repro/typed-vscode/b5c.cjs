// Batch 5, the rest: b5b's cases from the tables on, then b5's from Babel on.
const b5 = require('./b5.cjs');
const b5b = require('./b5b.cjs');

module.exports = [...b5b.slice(b5b.findIndex((c) => c.name === 'tables')), ...b5.slice(b5.findIndex((c) => c.name === 'babel'))];
