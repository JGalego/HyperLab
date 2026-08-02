/**
 * The one HTTP transport a page has, in the two tempos the runtime needs.
 *
 * Both functions answer with the same JSON envelope the WebAssembly module
 * expects — `{"status": …, "body": …}` — and throw only when the request
 * never got an answer at all. A refusal (401, 429, a vendor error) is a
 * *reply*, and reading it is the wire protocol's job in Rust, not ours.
 *
 * The request goes from this browser straight to the provider the user
 * configured. There is no server of ours in between — the site is static
 * files — which is the whole arrangement that makes a bring-your-own-key
 * playground honest.
 */

/** Asks and waits, for a script blocked mid-line on `ai("…")`. */
export function completeSync(url: string, headersJson: string, body: string): string {
  const request = new XMLHttpRequest();
  // Synchronous on purpose: `ai("…")` is an expression, and its value has
  // to reach the rest of the line. In the worker this blocks nothing the
  // user can see.
  request.open('POST', url, false);
  for (const [name, value] of Object.entries(
    JSON.parse(headersJson) as Record<string, string>,
  )) {
    request.setRequestHeader(name, value);
  }
  request.send(body);
  if (request.status === 0) {
    throw new Error('the request never got an answer');
  }
  return JSON.stringify({ status: request.status, body: request.responseText });
}

/** The same question without the waiting, for the assistant sidebar. */
export async function complete(
  url: string,
  headersJson: string,
  body: string,
): Promise<string> {
  const response = await fetch(url, {
    method: 'POST',
    headers: JSON.parse(headersJson) as Record<string, string>,
    body,
  });
  return JSON.stringify({
    status: response.status,
    body: await response.text(),
  });
}
