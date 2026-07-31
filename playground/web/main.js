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
  paint();
  evaluate();
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

  for (const control of [examples, runButton, formatButton]) {
    control.disabled = false;
  }

  source.addEventListener("input", paint);
  source.addEventListener("scroll", syncScroll);
  examples.addEventListener("change", () => loadExample(examples.value));
  runButton.addEventListener("click", evaluate);
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

  loadExample(example_names()[0]);
}

main().catch((error) => {
  output.textContent = `The playground failed to start.\n\n${error}`;
  output.classList.add("failed");
});
