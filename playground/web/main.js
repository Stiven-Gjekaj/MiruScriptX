// The playground's only script. It wires the page to the WebAssembly module and
// does nothing else: every question about the language is answered by calling
// into Rust, so nothing here can disagree with what `miru` does on a terminal.

import init, {
  run,
  format,
  disassemble,
  highlight,
  version,
  example_names,
  example_source,
} from "./pkg/miruscriptx_playground.js";

const source = document.getElementById("source");
const highlighted = document.querySelector("#highlight code");
const output = document.getElementById("output");
const examples = document.getElementById("examples");
const runButton = document.getElementById("run");
const formatButton = document.getElementById("format");
const shareButton = document.getElementById("share");
const outputTab = document.getElementById("tab-output");
const bytecodeTab = document.getElementById("tab-bytecode");

// Which tab is showing. The panes share one element, so this decides whether a
// run fills it with program output or with a disassembly.
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
}

/** Keep the layer behind the textarea scrolled to the same place. */
function syncScroll() {
  const pre = highlighted.parentElement;
  pre.scrollTop = source.scrollTop;
  pre.scrollLeft = source.scrollLeft;
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
}

/** Run or disassemble the current program, depending on the visible tab. */
function evaluate() {
  const program = source.value;
  show(view === "bytecode" ? disassemble(program) : run(program));
}

function selectTab(next) {
  view = next;
  const onOutput = next === "output";
  outputTab.classList.toggle("active", onOutput);
  bytecodeTab.classList.toggle("active", !onOutput);
  outputTab.setAttribute("aria-selected", String(onOutput));
  bytecodeTab.setAttribute("aria-selected", String(!onOutput));
  evaluate();
}

function loadExample(name) {
  source.value = example_source(name);
  // The example wins over whatever a shared link carried in, and clearing the
  // fragment stops the old program coming back on the next reload.
  clearFragment();
  paint();
  evaluate();
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
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
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

async function share() {
  let link;
  try {
    const encoded = await encodeProgram(source.value);
    link = `${location.origin}${location.pathname}#code=${encoded}`;
  } catch (error) {
    output.textContent = `Could not make a link.\n\n${error}`;
    output.classList.add("failed");
    return;
  }

  // Refused rather than truncated. A link that quietly loses the end of a
  // program is worse than no link: whoever opens it sees a program that looks
  // whole and is not.
  if (link.length > MAX_URL) {
    output.textContent =
      `This program is too long to put in a link (${link.length} characters, ` +
      `and about ${MAX_URL} is the most a link carries).\n\n` +
      "Send the program itself instead.";
    output.classList.add("failed");
    selectTab("output");
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
    paint();
    evaluate();
    return true;
  } catch {
    // A damaged fragment shows an empty editor and says so. Showing a partial
    // program as though it were whole is the one outcome to avoid.
    source.value = "";
    paint();
    output.textContent =
      "This link is damaged, so the program it carried could not be read.";
    output.classList.add("failed");
    selectTab("output");
    return true;
  }
}

async function main() {
  await init();

  document.getElementById("version").textContent = "";
  document.querySelector(".version").textContent = `MiruScriptX ${version()}`;

  for (const name of example_names()) {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    examples.append(option);
  }

  for (const control of [examples, runButton, formatButton, shareButton]) {
    control.disabled = false;
  }

  source.addEventListener("input", paint);
  source.addEventListener("scroll", syncScroll);
  examples.addEventListener("change", () => loadExample(examples.value));
  runButton.addEventListener("click", evaluate);
  shareButton.addEventListener("click", share);
  outputTab.addEventListener("click", () => selectTab("output"));
  bytecodeTab.addEventListener("click", () => selectTab("bytecode"));

  formatButton.addEventListener("click", () => {
    const formatted = format(source.value);
    if (formatted.ok) {
      source.value = formatted.text;
      paint();
      evaluate();
    } else {
      // A program that does not parse cannot be formatted. Show why rather
      // than silently doing nothing.
      show(formatted);
    }
  });

  // Ctrl-Enter and Cmd-Enter run, which is what every editor-shaped box on the
  // web has trained people to expect.
  source.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      evaluate();
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
