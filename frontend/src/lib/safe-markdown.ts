/** Escape-first local Markdown rendering with no remote media or raw HTML. */

/** Escapes every character that can enter an HTML tag or attribute. */
function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Renders a bounded inline Markdown subset after HTML has been escaped. */
function renderInline(value: string): string {
  /** Image syntax is reduced to its escaped alternative text and never becomes a request. */
  const withoutImages = value.replace(/!\[([^\]]*)\]\([^)]+\)/g, "$1");
  /** Raw text is escaped before any known-safe tags are introduced. */
  const escaped = escapeHtml(withoutImages);
  /** Code spans are rendered without interpreting their escaped contents. */
  const withCode = escaped.replace(/\x60([^\x60\n]+)\x60/g, "<code>$1</code>");
  /** Strong emphasis is limited to a single-line non-empty span. */
  const withStrong = withCode.replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>");
  /** Same-origin paths are the only link destinations that become anchors. */
  return withStrong.replace(/\[([^\]\n]+)\]\((\/(?!\/)[^)\s]*)\)/g, '<a href="$2">$1</a>');
}

/** Converts local Markdown source into a small sanitized HTML subset. */
export function renderSafeMarkdown(markdown: string): string {
  /** Normalized source lines used for deterministic block rendering. */
  const lines = markdown.replaceAll("\r\n", "\n").replaceAll("\r", "\n").split("\n");
  /** Safe rendered blocks collected without accepting raw source tags. */
  const blocks: string[] = [];
  /** Consecutive list items waiting to be wrapped in one list element. */
  let listItems: string[] = [];

  /** Flushes the current list before a different block type begins. */
  function flushList(): void {
    if (listItems.length === 0) return;
    blocks.push("<ul>" + listItems.map((item) => "<li>" + item + "</li>").join("") + "</ul>");
    listItems = [];
  }

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) {
      flushList();
      continue;
    }
    if (trimmed.startsWith("### ")) {
      flushList();
      blocks.push("<h4>" + renderInline(trimmed.slice(4)) + "</h4>");
      continue;
    }
    if (trimmed.startsWith("## ")) {
      flushList();
      blocks.push("<h3>" + renderInline(trimmed.slice(3)) + "</h3>");
      continue;
    }
    if (trimmed.startsWith("# ")) {
      flushList();
      blocks.push("<h2>" + renderInline(trimmed.slice(2)) + "</h2>");
      continue;
    }
    if (trimmed.startsWith("- ")) {
      listItems.push(renderInline(trimmed.slice(2)));
      continue;
    }
    flushList();
    blocks.push("<p>" + renderInline(trimmed) + "</p>");
  }
  flushList();
  return blocks.join("");
}
