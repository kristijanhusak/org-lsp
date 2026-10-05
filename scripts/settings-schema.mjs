// The server's options as JSON Schema, from what `org-lsp settings` prints:
// for VS Code's settings (editors/vscode/scripts/generate.mjs) and for the
// schema of the shared configuration file (scripts/generate-docs.mjs).

/** A copy of `schema` without the keys only this project reads. */
function clean(schema) {
  if (Array.isArray(schema)) return schema.map(clean);
  if (!schema || typeof schema !== 'object') return schema;
  return Object.fromEntries(
    Object.entries(schema)
      .filter(([key]) => !key.startsWith('x-'))
      .map(([key, value]) => [key, clean(value)]),
  );
}

/** What a description says about a setting: its meaning and Org's name for it. */
export function describe(setting) {
  return setting.org ? `${setting.doc} (\`${setting.org}\`)` : setting.doc;
}

/**
 * The options as the properties of a JSON Schema object, nested as the
 * options are (`agenda.span` under `agenda`). `editor`: with the options
 * only an editor gives (`configFile`). Integration options are left out.
 */
export function schemaProperties(settings, { editor = true, markdown = false } = {}) {
  const top = {};
  for (const setting of settings) {
    if (setting.integration || (!editor && setting.editorOnly) || !setting.schema) continue;
    const parts = setting.name.split('.');
    let properties = top;
    for (const part of parts.slice(0, -1)) {
      properties[part] ??= { type: 'object', properties: {} };
      properties = properties[part].properties;
    }
    const property = clean(setting.schema);
    property[markdown ? 'markdownDescription' : 'description'] = describe(setting);
    if (setting.default !== undefined) property.default = setting.default;
    properties[parts.at(-1)] = property;
  }
  return top;
}
