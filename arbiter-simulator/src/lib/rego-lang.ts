// Rego language support for CodeMirror 6.
// Based on the official Rego TextMate grammar.

import { StreamLanguage } from '@codemirror/language';
import { tags } from '@lezer/highlight';

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

const KEYWORDS = new Set([
  'default', 'not', 'package', 'import', 'as', 'with', 'else',
  'some', 'in', 'every', 'if', 'contains',
]);

const CONSTANTS = new Set(['true', 'false', 'null']);

const ROOT_DOCUMENTS = new Set(['input', 'data']);

function isDigit(ch: number): boolean {
  return ch >= 48 && ch <= 57;
}

function isAlpha(ch: number): boolean {
  return (ch >= 65 && ch <= 90) || (ch >= 97 && ch <= 122);
}

function isIdentStart(ch: number): boolean {
  return isAlpha(ch) || ch === 95;
}

function isIdentPart(ch: number): boolean {
  return isIdentStart(ch) || isDigit(ch);
}

export const regoLanguage = StreamLanguage.define<unknown>({
  name: 'rego',

  startState() {
    return {};
  },

  token(stream: any, _state: unknown) {
    // Whitespace
    if (stream.eatSpace()) return null;

    // -------------------------------------------------------------------
    // Comments
    // -------------------------------------------------------------------
    if (stream.peek() === 35 /* '#' */) {
      stream.next();
      const rest = stream.string.slice(stream.pos);
      if (/^\s*(METADATA|\w+:)/.test(rest)) {
        stream.skipToEnd();
        return tags.meta;
      }
      stream.skipToEnd();
      return tags.lineComment;
    }

    // -------------------------------------------------------------------
    // Strings: double-quoted "..."
    // -------------------------------------------------------------------
    if (stream.peek() === 34 /* '"' */) {
      stream.next();
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === 34) return tags.string;
        if (ch === 92) stream.next(); // skip escape
      }
      return tags.string;
    }

    // -------------------------------------------------------------------
    // Strings: raw backtick `...`
    // -------------------------------------------------------------------
    if (stream.peek() === 96 /* '`' */) {
      stream.next();
      while (!stream.eol()) {
        if (stream.next() === 96) return tags.string;
      }
      return tags.string;
    }

    // -------------------------------------------------------------------
    // Interpolated strings: $"..." and $`...`
    // -------------------------------------------------------------------
    if (stream.peek() === 36 /* '$' */ && stream.pos + 1 < stream.string.length) {
      const nextCh = stream.string.charCodeAt(stream.pos + 1);
      if (nextCh === 34 || nextCh === 96) {
        stream.next(); // consume '$'
        stream.next(); // consume opening quote
        const closeCh = nextCh === 34 ? 34 : 96;
        while (!stream.eol()) {
          const ch = stream.next();
          if (ch === closeCh) return tags.string;
          if (ch === 123 /* '{' */) {
            let depth = 1;
            while (depth > 0 && !stream.eol()) {
              const c = stream.next();
              if (c === 123) depth++;
              if (c === 125) depth--;
            }
          }
          if (ch === 92) stream.next();
        }
        return tags.string;
      }
    }

    // -------------------------------------------------------------------
    // Numbers
    // -------------------------------------------------------------------
    if (stream.peek() === 45 /* '-' */ || isDigit(stream.peek())) {
      const start = stream.pos;
      stream.nextIf(45);
      if (isDigit(stream.peek())) {
        stream.eatWhile(isDigit);
        if (stream.nextIf(46)) stream.eatWhile(isDigit); // '.'
        if (stream.peek() === 69 || stream.peek() === 101) {
          stream.next();
          stream.nextIf(43) || stream.nextIf(45);
          if (isDigit(stream.peek())) stream.eatWhile(isDigit);
        }
        return tags.number;
      }
      stream.pos = start;
    }

    // -------------------------------------------------------------------
    // Identifiers and keywords
    // -------------------------------------------------------------------
    if (isIdentStart(stream.peek())) {
      const start = stream.pos;
      stream.eatWhile(isIdentPart);

      // Dotted identifiers (builtins like arbiter.get_space_members)
      while (stream.peek() === 46 /* '.' */ && isIdentStart(stream.string.charCodeAt(stream.pos + 1))) {
        stream.next();
        stream.eatWhile(isIdentPart);
      }

      const word = stream.string.slice(start, stream.pos);

      if (KEYWORDS.has(word)) return tags.keyword;
      if (CONSTANTS.has(word)) return tags.atom;
      if (ROOT_DOCUMENTS.has(word) && stream.peek() !== 46) return tags.definition;
      if (stream.peek() === 40 /* '(' */) return tags.function(tags.variableName);
      if (stream.peek() === 58) {
        if (stream.string.charCodeAt(stream.pos + 1) === 61) return tags.definition;
      }
      if (stream.peek() === 91 || stream.peek() === 123) return tags.definition;

      return null;
    }

    // -------------------------------------------------------------------
    // Operators
    // -------------------------------------------------------------------
    const ch = stream.peek();
    if (ch === 61) {
      stream.next();
      if (stream.peek() === 61) stream.next();
      return tags.operator;
    }
    if (ch === 33) {
      stream.next();
      if (stream.peek() === 61) stream.next();
      return tags.operator;
    }
    if (ch === 62 || ch === 60) {
      stream.next();
      if (stream.peek() === 61) stream.next();
      return tags.operator;
    }
    if (ch === 43 || ch === 45 || ch === 42 || ch === 37 || ch === 47) {
      stream.next();
      return tags.operator;
    }
    if (ch === 124 || ch === 38) {
      stream.next();
      return tags.operator;
    }
    if (ch === 58) {
      stream.next();
      if (stream.peek() === 61) stream.next();
      return tags.operator;
    }

    // -------------------------------------------------------------------
    // Punctuation
    // -------------------------------------------------------------------
    if ('()[]{}.,;'.includes(String.fromCharCode(ch))) {
      stream.next();
      return tags.punctuation;
    }

    stream.next();
    return null;
  },

  languageData: {
    commentTokens: { line: '#' },
    closeBrackets: { brackets: ['(', '[', '{', '"'] },
  },
});
