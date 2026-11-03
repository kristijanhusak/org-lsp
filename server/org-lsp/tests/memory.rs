//! How much memory the server takes on documents made to make it take
//! much. An allocator counts the bytes each thread holds, and the most it
//! held.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use org_lsp::highlight::{tokens, Conceal};
use org_syntax::TodoKeywords;

struct Counting;

thread_local! {
    /// The bytes this thread holds, and the most it held.
    static HELD: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = HELD.try_with(|held| {
            let (now, most) = held.get();
            held.set((now + layout.size(), most.max(now + layout.size())));
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = HELD.try_with(|held| {
            let (now, most) = held.get();
            held.set((now.saturating_sub(layout.size()), most));
        });
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// The most memory `run` held at once, beyond what was held before it.
fn most_held(run: impl FnOnce()) -> usize {
    let before = HELD.with(|held| {
        let (now, _) = held.get();
        held.set((now, now));
        now
    });
    run();
    HELD.with(|held| held.get().1 - before)
}

/// Semantic tokens of a deep outline: each headline used to copy its whole
/// subtree while its children were colored, memory cubic in the depth
/// (146 MB here, 630 times the text).
#[test]
fn tokens_of_a_deep_outline() {
    let mut text: String = (1..=400).map(|level| format!("{} h{level}\n", "*".repeat(level))).collect();
    text.push_str(&"text text text\n".repeat(10_000));
    let parse = org_syntax::parse(&text, &TodoKeywords::default());
    let root = parse.syntax();
    let held = most_held(|| {
        tokens(&root, parse.todo_keywords(), Conceal::default());
    });
    assert!(held < text.len(), "{held} bytes held for {} bytes of text", text.len());
}
