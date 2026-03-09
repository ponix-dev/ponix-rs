use yrs::{Doc, GetString, Transact};

/// Extract plaintext content from a Yrs document.
/// Reads from the root text type ("content") used by the Ponix editor.
pub fn extract_content_text(doc: &Doc) -> String {
    let text = doc.get_or_insert_text(common::yrs::ROOT_TEXT_NAME);
    let txn = doc.transact();
    text.get_string(&txn)
}

/// Extract HTML content from a Yrs document.
/// For now, wraps the plaintext in a paragraph tag. As the editor evolves
/// to use XmlFragment, this will walk the tree to produce structured HTML.
pub fn extract_content_html(doc: &Doc) -> String {
    let text = extract_content_text(doc);
    if text.is_empty() {
        return String::new();
    }
    // Split into paragraphs on double newlines, wrap each in <p>
    text.split("\n\n")
        .filter(|s| !s.is_empty())
        .map(|p| format!("<p>{}</p>", html_escape(p)))
        .collect::<Vec<_>>()
        .join("")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::yrs::create_empty_document;
    use yrs::updates::decoder::Decode;
    use yrs::{Text, Transact, Update};

    fn make_doc_with_text(content: &str) -> Doc {
        let doc = Doc::new();
        let (state, _) = create_empty_document();
        {
            let mut txn = doc.transact_mut();
            let update = Update::decode_v1(&state).unwrap();
            txn.apply_update(update).unwrap();
        }
        {
            let text = doc.get_or_insert_text(common::yrs::ROOT_TEXT_NAME);
            let mut txn = doc.transact_mut();
            text.insert(&mut txn, 0, content);
        }
        doc
    }

    #[test]
    fn test_extract_empty_doc() {
        let doc = Doc::new();
        let _ = doc.get_or_insert_text(common::yrs::ROOT_TEXT_NAME);
        assert_eq!(extract_content_text(&doc), "");
        assert_eq!(extract_content_html(&doc), "");
    }

    #[test]
    fn test_extract_text_content() {
        let doc = make_doc_with_text("hello world");
        assert_eq!(extract_content_text(&doc), "hello world");
    }

    #[test]
    fn test_extract_html_single_paragraph() {
        let doc = make_doc_with_text("hello world");
        assert_eq!(extract_content_html(&doc), "<p>hello world</p>");
    }

    #[test]
    fn test_extract_html_multiple_paragraphs() {
        let doc = make_doc_with_text("para one\n\npara two");
        assert_eq!(extract_content_html(&doc), "<p>para one</p><p>para two</p>");
    }

    #[test]
    fn test_extract_html_escapes_special_chars() {
        let doc = make_doc_with_text("<script>alert('xss')</script>");
        let html = extract_content_html(&doc);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
