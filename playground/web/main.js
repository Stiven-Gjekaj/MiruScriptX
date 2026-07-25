// The playground's only script. It wires the page to the WebAssembly module and
// does nothing else: every question about the language is answered by calling
// into Rust, so nothing here can disagree with what `miru` does on a terminal.

import init, {
  run,
  format,
  disassemble,
  version,
  example_names,
  example_source,
} from "./pkg/miruscriptx_playground.js";

const source = document.getElementById("source");
const highlight = document.querySelector("#highlight code");
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
 * Repaint the layer behind the textarea.
 *
 * For now this only mirrors the text, which keeps the two layers aligned and
 * proves the geometry is right before any colour is involved. Highlighting
 * replaces the body of this function and nothing else.
 *
 * The trailing newline matters: a <pre> collapses one at the end, so without it
 * the last line would sit half a line higher than the textarea's.
 */
function paint() {
  highlight.textContent = source.value + "\n";
}

/** Keep the layer behind the textarea scrolled to the same place. */
function syncScroll() {
  const pre = highlight.parentElement;
  pre.scrollTop = source.scrollTop;
  pre.scrollLeft = source.scrollLeft;
}

/** Show an outcome, marking failure so the styling can distinguish it. */
function show(outcome) {
  output.textContent = outcome.text;
  output.classList.toggle("failed", !outcome.ok);
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
