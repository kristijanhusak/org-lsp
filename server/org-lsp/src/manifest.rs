//! Everything an editor integration needs to know about the server that
//! LSP has no field for: the commands with their default keys, how tokens
//! look by default, and how the agenda document is named.
//!
//! The server sends this as `capabilities.experimental.org` in the
//! `initialize` result, and `org-lsp manifest` prints it for integrations
//! that are generated ahead of time. `docs/protocol.md` describes it.

use serde_json::{json, Value};

use crate::{commands, legend, prompt, views};

pub fn manifest() -> Value {
    let mut manifest = json!({
        "commands": commands::manifest(),
        "agenda": {
            // The scheme of the agenda document, for clients that fetch it
            // with `workspace/textDocumentContent`.
            "scheme": views::SCHEME,
            "uri": views::VIRTUAL_URI,
            // The name of the file other clients are shown instead.
            "fileName": views::FILE_NAME,
            // The language identifier of the agenda document.
            "languageId": views::LANGUAGE,
        },
        // Org's date prompt: a calendar and the answer typed so far, for
        // clients that announce `experimental.orgDatePrompt`. Its keys are
        // the commands' keys `in` the `calendar` scope.
        "datePrompt": {
            "scheme": views::SCHEME,
            "uri": prompt::URI,
            "languageId": prompt::LANGUAGE,
        },
    });
    if let (Some(manifest), Value::Object(legend)) = (manifest.as_object_mut(), legend::manifest()) {
        manifest.extend(legend);
    }
    manifest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_every_section() {
        let manifest = manifest();
        for section in ["commands", "agenda", "datePrompt", "tokenTypes", "tokenModifiers", "highlights"] {
            assert!(!manifest[section].is_null(), "missing `{section}`");
        }
        assert_eq!(manifest["agenda"]["scheme"], "org-agenda");
    }
}
