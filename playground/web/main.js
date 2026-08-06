// The playground's only script. It wires the page to the WebAssembly module and
// does nothing else: every question about the language is answered by calling
// into Rust, so nothing here can disagree with what `miru` does on a terminal.

import init, {
  run,
  format,
  disassemble,
  highlight,
  version,
  examples as exampleCards,
  example_names,
  example_source,
} from "./pkg/miruscriptx_playground.js";

const source = document.getElementById("source");
const highlighted = document.querySelector("#highlight code");
const gutter = document.getElementById("gutter");
const output = document.getElementById("output");
const reference = document.getElementById("reference");
const cards = document.getElementById("examples");
const filename = document.getElementById("filename");
const runButton = document.getElementById("run");
const formatButton = document.getElementById("format");
const shareButton = document.getElementById("share");
const themeButton = document.getElementById("theme");
const outputTab = document.getElementById("tab-output");
const bytecodeTab = document.getElementById("tab-bytecode");
const referenceTab = document.getElementById("tab-reference");
const statusVersion = document.getElementById("status-version");
const statusExit = document.getElementById("status-exit");
const statusMs = document.getElementById("status-ms");
const statusLines = document.getElementById("status-lines");
const buildBadge = document.getElementById("build");

// Which tab is showing. Output and bytecode share one element, so this decides
// whether a run fills it with program output or with a disassembly.
let view = "output";

/**
 * Repaint the layer behind the textarea, colouring it.
 *
 * The spans come from the real lexer, so what gets coloured as a keyword is
 * exactly what the language treats as one. Nothing here knows the grammar.
 *
 * Offsets are char indices, which is why the text is split into an array of
 * code points first: indexing a JavaScript string directly counts UTF-16 units,
 * and a single astral character would shift every span after it.
 *
 * The trailing newline matters: a <pre> collapses one at the end, so without it
 * the last line would sit half a line higher than the textarea's.
 */
function paint() {
  const text = source.value;
  const chars = Array.from(text);
  const fragment = document.createDocumentFragment();
  let at = 0;

  const plain = (upto) => {
    if (upto > at) {
      fragment.append(chars.slice(at, upto).join(""));
      at = upto;
    }
  };

  for (const span of highlight(text)) {
    // Defensive: the spans are sorted and disjoint, and a test in the Rust
    // crate keeps them that way, but skipping a stray overlap loses colour
    // whereas honouring one would duplicate text.
    if (span.start < at) continue;
    plain(span.start);
    const element = document.createElement("span");
    element.className = span.class;
    element.textContent = chars
      .slice(span.start, span.start + span.length)
      .join("");
    fragment.append(element);
    at = span.start + span.length;
  }
  plain(chars.length);
  fragment.append("\n");

  highlighted.replaceChildren(fragment);
  paintGutter();
}

/**
 * Number the lines beside the editor.
 *
 * Counted the way a reader counts: a trailing newline ends the last line
 * rather than starting an empty one, which is what `line_count` in the crate
 * does for the example cards. The two agree on purpose.
 */
function paintGutter() {
  const text = source.value;
  const count = text === "" ? 1 : text.replace(/\n$/, "").split("\n").length;
  let numbers = "";
  for (let line = 1; line <= count; line += 1) {
    numbers += `${line}\n`;
  }
  gutter.textContent = numbers;
  statusLines.textContent = `${count} ${count === 1 ? "line" : "lines"}`;
}

/** Keep the two layers behind the textarea scrolled to the same place. */
function syncScroll() {
  const pre = highlighted.parentElement;
  pre.scrollTop = source.scrollTop;
  pre.scrollLeft = source.scrollLeft;
  gutter.scrollTop = source.scrollTop;
}

/**
 * Show an outcome, marking failure so the styling can distinguish it.
 *
 * A program has two streams and may stop with a code. `failed` means an error
 * ended the run, not that the code was non-zero: a program that exits 2 has
 * produced real output and should not be painted as a crash.
 */
function show(outcome) {
  output.textContent = "";
  output.classList.toggle("failed", !outcome.ok);

  if (outcome.text) {
    output.append(outcome.text);
  }
  const notes = outcome.diagnostics ?? "";
  if (notes) {
    const stream = document.createElement("span");
    stream.className = "diagnostics";
    stream.textContent = notes;
    output.append(stream);
  }
  const code = outcome.exit_code ?? 0;
  if (code !== 0) {
    const note = document.createElement("span");
    note.className = "exit";
    note.textContent = `\nthe program stopped with code ${code}\n`;
    output.append(note);
  }
  statusExit.textContent = outcome.ok ? `exit ${code}` : "failed";
}

/**
 * Run or disassemble the current program, depending on the visible tab.
 *
 * Timed with `performance.now`, which is the page's own clock rather than the
 * language's: this is how long the call took here, not something a program can
 * observe about itself. Disassembling is timed the same way, because it is
 * still work the page did and the reader asked for.
 */
function evaluate() {
  const program = source.value;
  const started = performance.now();
  const outcome = view === "bytecode" ? disassemble(program) : run(program);
  const elapsed = performance.now() - started;

  show(outcome);
  output.classList.toggle("bytecode", view === "bytecode");
  statusMs.textContent = `${elapsed.toFixed(elapsed < 10 ? 1 : 0)} ms`;
}

/**
 * Show one of the three tabs.
 *
 * Reference is not a result, so it neither runs the program nor disturbs what
 * the last run said. Output and bytecode are two views of the same program and
 * re-evaluate, which is what makes switching to bytecode show the bytecode of
 * what is in the editor right now.
 */
function selectTab(next) {
  view = next;
  const tabs = {
    output: outputTab,
    bytecode: bytecodeTab,
    reference: referenceTab,
  };
  for (const [name, tab] of Object.entries(tabs)) {
    const on = name === next;
    tab.classList.toggle("active", on);
    tab.setAttribute("aria-selected", String(on));
  }

  const onReference = next === "reference";
  reference.hidden = !onReference;
  output.hidden = onReference;
  if (!onReference) {
    evaluate();
  }
}

function loadExample(name) {
  source.value = example_source(name);
  filename.textContent = `${name}.miru`;
  for (const card of cards.querySelectorAll(".card")) {
    card.classList.toggle("current", card.dataset.name === name);
  }
  // The example wins over whatever a shared link carried in, and clearing the
  // fragment stops the old program coming back on the next reload.
  clearFragment();
  paint();
  if (view === "reference") {
    selectTab("output");
  } else {
    evaluate();
  }
}

/**
 * Build the example cards.
 *
 * The tag, the description, and the line count all come from the crate, so a
 * card cannot describe a program the repository does not have. The page only
 * decides how they are arranged.
 */
function buildCards() {
  const fragment = document.createDocumentFragment();
  for (const example of exampleCards()) {
    const card = document.createElement("button");
    card.type = "button";
    card.className = "card";
    card.dataset.name = example.name;

    const tag = document.createElement("span");
    tag.className = "card-tag";
    tag.textContent = example.tag;

    const name = document.createElement("span");
    name.className = "card-name";
    name.textContent = example.name;

    const about = document.createElement("span");
    about.className = "card-about";
    about.textContent = example.about;

    const lines = document.createElement("span");
    lines.className = "card-lines";
    lines.textContent = `${example.lines} lines`;

    card.append(tag, name, about, lines);
    card.addEventListener("click", () => loadExample(example.name));
    fragment.append(card);
  }

  const wiki = document.createElement("a");
  wiki.className = "card card-wiki";
  wiki.href = "https://github.com/stiven-gjekaj/miruscriptx/tree/main/wiki";
  wiki.innerHTML =
    '<span class="card-tag">Keep going</span>' +
    '<span class="card-name">The wiki teaches the rest</span>' +
    '<span class="card-about">Eighteen short lessons, read in order.</span>' +
    '<span class="card-lines">18 lessons →</span>';
  fragment.append(wiki);

  cards.replaceChildren(fragment);
}

// --- The theme --------------------------------------------------------------
//
// The system's preference decides until somebody chooses, and then the choice
// is remembered. Setting the attribute in both directions is what lets a
// person read in light while their machine is in dark.

function currentlyDark() {
  const chosen = document.documentElement.dataset.theme;
  if (chosen) {
    return chosen === "dark";
  }
  return matchMedia("(prefers-color-scheme: dark)").matches;
}

function describeTheme() {
  themeButton.setAttribute(
    "aria-label",
    currentlyDark() ? "Switch to the light theme" : "Switch to the dark theme",
  );
}

function toggleTheme() {
  const next = currentlyDark() ? "light" : "dark";
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem("miru-theme", next);
  } catch {
    // Private browsing can refuse to store it. The page still changes; it just
    // will not remember, which is better than refusing to change at all.
  }
  describeTheme();
}

// --- Sharing ----------------------------------------------------------------
//
// The program goes in the URL fragment, which never leaves the browser: the
// server is not sent it and GitHub Pages keeps no record of it. A query string
// would be sent on every request, and a program is the writer's own text.
//
// Compressed before encoding, because base64 alone makes text a third larger
// and a URL has a practical ceiling. CompressionStream is a browser API, so
// this page still has no dependencies: everything under playground/web/ is
// hand-written HTML, CSS, and JavaScript and stays that way.

/** Roughly what a browser will carry. Beyond this a link is refused. */
const MAX_URL = 8000;

/** Base64url: the plain alphabet loses `+` and `/` to URL escaping. */
function toBase64Url(bytes) {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}

function fromBase64Url(text) {
  const padded = text.replaceAll("-", "+").replaceAll("_", "/");
  const binary = atob(padded);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

async function collect(stream) {
  const chunks = [];
  const reader = stream.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
  }
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const bytes = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.length;
  }
  return bytes;
}

async function encodeProgram(text) {
  // The UTF-8 bytes, not the string. `btoa` refuses a character above U+00FF,
  // so a program with an emoji in it would fail without this step.
  const bytes = new TextEncoder().encode(text);
  const stream = new Blob([bytes])
    .stream()
    .pipeThrough(new CompressionStream("deflate-raw"));
  return toBase64Url(await collect(stream));
}

async function decodeProgram(encoded) {
  const bytes = fromBase64Url(encoded);
  const stream = new Blob([bytes])
    .stream()
    .pipeThrough(new DecompressionStream("deflate-raw"));
  return new TextDecoder().decode(await collect(stream));
}

function clearFragment() {
  if (location.hash) {
    history.replaceState(null, "", location.pathname + location.search);
  }
}

/** Say something in the output pane, and make sure it is the pane on show. */
function complain(text) {
  selectTab("output");
  output.textContent = text;
  output.classList.add("failed");
  output.classList.remove("bytecode");
}

async function share() {
  let link;
  try {
    const encoded = await encodeProgram(source.value);
    link = `${location.origin}${location.pathname}#code=${encoded}`;
  } catch (error) {
    complain(`Could not make a link.\n\n${error}`);
    return;
  }

  // Refused rather than truncated. A link that quietly loses the end of a
  // program is worse than no link: whoever opens it sees a program that looks
  // whole and is not.
  if (link.length > MAX_URL) {
    complain(
      `This program is too long to put in a link (${link.length} characters, ` +
        `and about ${MAX_URL} is the most a link carries).\n\n` +
        "Send the program itself instead.",
    );
    return;
  }

  history.replaceState(null, "", link);
  try {
    await navigator.clipboard.writeText(link);
    shareButton.textContent = "Copied";
  } catch {
    // No clipboard permission, which some browsers withhold. The link is in
    // the address bar either way, so say that rather than failing.
    shareButton.textContent = "In the address bar";
  }
  setTimeout(() => {
    shareButton.textContent = "Share";
  }, 2000);
}

/**
 * Load a program from the fragment, if there is one.
 *
 * Gives whether one was loaded, so start-up knows to skip the first example.
 */
async function loadFromFragment() {
  const match = /^#code=(.+)$/.exec(location.hash);
  if (!match) {
    return false;
  }
  try {
    source.value = await decodeProgram(match[1]);
    filename.textContent = "shared.miru";
    paint();
    evaluate();
    return true;
  } catch {
    // A damaged fragment shows an empty editor and says so. Showing a partial
    // program as though it were whole is the one outcome to avoid.
    source.value = "";
    paint();
    complain("This link is damaged, so the program it carried could not be read.");
    return true;
  }
}

/**
 * Fill the keyword chips from the language rather than from a list.
 *
 * Section 2.5 of the specification is the source: sixteen words, and the
 * lexer will colour exactly these. Asking the highlighter which of them it
 * calls a keyword would be circular, so this asks the simpler question, that
 * every one of them highlights as something rather than as a plain name.
 */
function buildKeywords() {
  const words = [
    "fn",
    "let",
    "return",
    "if",
    "else",
    "while",
    "for",
    "in",
    "break",
    "continue",
    "import",
    "as",
    "try",
    "true",
    "false",
    "nil",
  ];
  const fragment = document.createDocumentFragment();
  for (const word of words) {
    const chip = document.createElement("span");
    chip.textContent = word;
    fragment.append(chip);
  }
  document.getElementById("keywords").replaceChildren(fragment);
}

async function main() {
  await init();

  const release = version();
  statusVersion.textContent = `miru ${release}`;
  buildBadge.textContent = `v${release} · wasm`;

  // A Mac says Cmd, everything else says Ctrl, and the status bar should tell
  // people which one their own machine wants.
  if (navigator.platform?.startsWith("Mac") || /Mac/.test(navigator.userAgent)) {
    document.getElementById("run-key").textContent = "⌘";
  }

  buildCards();
  buildKeywords();

  for (const control of [runButton, formatButton, shareButton]) {
    control.disabled = false;
  }

  source.addEventListener("input", paint);
  source.addEventListener("scroll", syncScroll);
  runButton.addEventListener("click", () => {
    if (view === "reference") {
      selectTab("output");
    } else {
      evaluate();
    }
  });
  shareButton.addEventListener("click", share);
  themeButton.addEventListener("click", toggleTheme);
  outputTab.addEventListener("click", () => selectTab("output"));
  bytecodeTab.addEventListener("click", () => selectTab("bytecode"));
  referenceTab.addEventListener("click", () => selectTab("reference"));
  describeTheme();

  formatButton.addEventListener("click", () => {
    const formatted = format(source.value);
    if (formatted.ok) {
      source.value = formatted.text;
      paint();
      if (view === "reference") {
        selectTab("output");
      } else {
        evaluate();
      }
    } else {
      // A program that does not parse cannot be formatted. Show why rather
      // than silently doing nothing.
      selectTab("output");
      show(formatted);
    }
  });

  // Ctrl-Enter and Cmd-Enter run, which is what every editor-shaped box on the
  // web has trained people to expect.
  source.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      if (view === "reference") {
        selectTab("output");
      } else {
        evaluate();
      }
    }
  });

  // A shared link wins over the first example, which is the whole point of
  // opening one.
  if (!(await loadFromFragment())) {
    loadExample(example_names()[0]);
  }
}

main().catch((error) => {
  output.textContent = `The playground failed to start.\n\n${error}`;
  output.classList.add("failed");
});
