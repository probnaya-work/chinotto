/**
 * Recognition on this Mac, or not at all — in every build.
 *
 * `SFSpeechRecognizer` sends audio to Apple's servers unless the request *requires*
 * on-device recognition **and** the recogniser *supports* it; Apple ignores the requirement
 * where support is missing. The direct build used to go ahead regardless, which is a server
 * path; only the App Store build refused. Nothing a test can run exercises Speech, so this
 * reads `speech.rs` and holds it to the shape that makes a server path impossible.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const rust = readFileSync(join(__dirname, "..", "..", "src-tauri", "src", "speech.rs"), "utf8");
/** Comments say what the code must not do; only the code is held to it. */
const code = rust
  .split("\n")
  .filter((line) => !/^\s*\/\//.test(line))
  .join("\n");

const count = (needle: string | RegExp) =>
  typeof needle === "string" ? code.split(needle).length - 1 : (code.match(needle) ?? []).length;

function bodyOf(signature: string): string {
  const start = code.indexOf(signature);
  expect(start).toBeGreaterThan(-1);
  let depth = 0;
  for (let i = code.indexOf("{", start); i < code.length; i += 1) {
    if (code[i] === "{") depth += 1;
    if (code[i] === "}") depth -= 1;
    if (depth === 0) return code.slice(start, i + 1);
  }
  throw new Error(`unterminated ${signature}`);
}

describe("Mac voice recognition stays on the device", () => {
  const gate = bodyOf("fn on_device_task(");

  it("creates a recognition task in exactly one place", () => {
    expect(count("recognitionTaskWithRequest_resultHandler(")).toBe(1);
    expect(gate).toContain("recognitionTaskWithRequest_resultHandler(");
  });

  it("checks support and sets the requirement before that task exists", () => {
    const support = gate.indexOf("if !unsafe { recognizer.supportsOnDeviceRecognition() }");
    const require = gate.indexOf("request.setRequiresOnDeviceRecognition(true)");
    const confirm = gate.indexOf("if !unsafe { request.requiresOnDeviceRecognition() }");
    const task = gate.indexOf("recognitionTaskWithRequest_resultHandler(");
    expect(support).toBeGreaterThan(-1);
    expect(require).toBeGreaterThan(support);
    expect(confirm).toBeGreaterThan(require);
    expect(task).toBeGreaterThan(confirm);
  });

  it("is the same in the App Store and direct builds", () => {
    expect(count('feature = "mas"')).toBe(0);
    expect(count(/setRequiresOnDeviceRecognition\(false\)/g)).toBe(0);
    expect(count("setRequiresOnDeviceRecognition(")).toBe(1);
  });

  it("sends every request through the gate", () => {
    expect(count("SFSpeechAudioBufferRecognitionRequest::init(")).toBe(1);
    expect(count("on_device_task(r, &request")).toBe(1);
  });

  it("hands audio to Speech only through a request the gate accepted", () => {
    expect(count("appendAudioPCMBuffer(")).toBe(1);
    const accept = code.indexOf("let task = on_device_task(r, &request, &result_block)?;");
    const kept = code.indexOf("*slot = Some(request);");
    expect(accept).toBeGreaterThan(-1);
    expect(kept).toBeGreaterThan(accept);
  });

  it("keeps recording when there is no local recogniser", () => {
    // The filter only drops the recogniser; the capture goes on to open the file.
    const capture = bodyOf("pub fn run_capture(");
    expect(capture).toContain("let recognizer = recognizer.filter(");
    expect(capture.indexOf("AVAudioFile::initForWriting_settings_error")).toBeGreaterThan(
      capture.indexOf("let recognizer = recognizer.filter("),
    );
  });
});
