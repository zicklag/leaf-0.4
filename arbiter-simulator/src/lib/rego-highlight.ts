// Lightweight Rego syntax highlighter.
// Generates HTML with span tags for syntax coloring.
// Based on the Rego TextMate grammar from vscode-opa-highlight-only.

interface Token {
  type: string;
  start: number;
  end: number;
}

const KEYWORDS = new Set([
  'package', 'import', 'as', 'default', 'not', 'if', 'else',
  'some', 'in', 'every', 'contains', 'with', 'true', 'false', 'null',
]);

const BUILTINS = new Set([
  'arbiter.get_space_members',
  'arbiter.get_space_config',
]);

/// Tokenize Rego source into colored spans.
export function highlightRego(source: string): string {
  const lines = source.split('\n');
  return lines.map((line) => highlightLine(line)).join('\n');
}

function highlightLine(line: string): string {
  const parts: string[] = [];
  let i = 0;

  while (i < line.length) {
    // Comment
    if (line[i] === '#') {
      parts.push(span('comment', line.slice(i)));
      break;
    }

    // String
    if (line[i] === '"') {
      const end = findStringEnd(line, i);
      parts.push(span('string', line.slice(i, end)));
      i = end;
      continue;
    }

    // Number
    if (isDigit(line[i]) || (line[i] === '-' && i + 1 < line.length && isDigit(line[i + 1]))) {
      const start = i;
      i++;
      while (i < line.length && (isDigit(line[i]) || line[i] === '.' || line[i] === 'e' || line[i] === 'E' || line[i] === '+' || line[i] === '-')) {
        i++;
      }
      parts.push(span('number', line.slice(start, i)));
      continue;
    }

    // Word (keyword, builtin, identifier, operator)
    if (isIdentStart(line[i])) {
      const start = i;
      i++;
      while (i < line.length && isIdentPart(line[i])) i++;
      const word = line.slice(start, i);

      // Check for builtin call (word followed by paren)
      if (BUILTINS.has(word) || isBuiltinCall(line, start, i)) {
        // Check if it's a multi-part builtin like arbiter.get_space_members
        const fullBuiltin = readFullBuiltin(line, start);
        if (fullBuiltin) {
          parts.push(span('builtin', fullBuiltin));
          i = start + fullBuiltin.length;
          continue;
        }
        parts.push(span('builtin', word));
        continue;
      }

      if (KEYWORDS.has(word)) {
        parts.push(span('keyword', word));
        continue;
      }

      // Check for function call: identifier(
      if (i < line.length && line[i] === '(') {
        parts.push(span('function', word));
        continue;
      }

      // Rule name: identifier followed by :=, [, or {
      if (i < line.length && (line[i] === ':' || line[i] === '[' || line[i] === '{')) {
        parts.push(span('rule', word));
        continue;
      }

      parts.push(span('identifier', word));
      continue;
    }

    // Operators
    if ('=!<>+-*/%|&:'.includes(line[i])) {
      const start = i;
      i++;
      // Multi-char operators: :=, ==, !=, >=, <=
      if (i < line.length && '=>'.includes(line[i])) i++;
      parts.push(span('operator', line.slice(start, i)));
      continue;
    }

    // Symbols / punctuation
    if ('()[]{}.,;|'.includes(line[i])) {
      parts.push(span('punctuation', line[i]));
      i++;
      continue;
    }

    // Everything else (whitespace, etc.)
    parts.push(escapeHtml(line[i]));
    i++;
  }

  return parts.join('');
}

function span(cls: string, text: string): string {
  return `<span class="hl-${cls}">${escapeHtml(text)}</span>`;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function isDigit(c: string): boolean {
  return c >= '0' && c <= '9';
}

function isIdentStart(c: string): boolean {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c === '_';
}

function isIdentPart(c: string): boolean {
  return isIdentStart(c) || isDigit(c) || c === '.';
}

function findStringEnd(line: string, start: number): number {
  let i = start + 1;
  while (i < line.length) {
    if (line[i] === '\\') {
      i += 2; // skip escape
    } else if (line[i] === '"') {
      return i + 1;
    } else {
      i++;
    }
  }
  return i;
}

function readFullBuiltin(line: string, start: number): string | null {
  // Read a dotted identifier like arbiter.get_space_members
  let i = start;
  while (i < line.length && (isIdentPart(line[i]) || line[i] === '.')) {
    i++;
  }
  const word = line.slice(start, i);
  if (BUILTINS.has(word)) return word;
  return null;
}

function isBuiltinCall(line: string, start: number, end: number): boolean {
  // Check if the word is followed by ( and preceded by nothing special
  const word = line.slice(start, end);
  if (BUILTINS.has(word)) return true;
  return false;
}
