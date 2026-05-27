// Rego/OPA language support for CodeMirror 6.
// Based on the official Rego Monarch grammar from Regorus:
// https://github.com/anakrish/regorus/blob/bef3c70dd387014dc3273064a6242d3d1bc4030f/rvm-playground/app.js#L967-L1025

import { StreamLanguage } from '@codemirror/language';

// ---------------------------------------------------------------------------
// Rego keywords & built-ins
// ---------------------------------------------------------------------------

const KEYWORDS = new Set([
  'default', 'not', 'package', 'import', 'as', 'with', 'else',
  'some', 'in', 'every', 'if', 'contains',
]);

const CONSTANTS = new Set(['true', 'false', 'null']);

const ROOT_DOCUMENTS = new Set(['data', 'input']);

// Rego built-in functions (excluding xrpc_* which are Muni Town specific)
const BUILTINS = new Set([
  // Aggregation
  'count', 'sum', 'max', 'min', 'product', 'any', 'all', 'sort', 'union', 'intersection',
  // Strings
  'sprintf', 'concat', 'replace', 'split', 'trim', 'trim_left', 'trim_right',
  'trim_prefix', 'trim_suffix', 'substring', 'contains', 'startswith', 'endswith',
  'indexof', 'last_indexof', 'lower', 'upper', 'format_int', 'reverse',
  // Regex
  're_match', 'regex.match', 'regex.split', 'regex.find', 'regex.find_n', 'regex.replace',
  'glob.match', 'glob.quote_meta',
  // Numbers
  'round', 'ceil', 'floor', 'abs',
  'numbers.range', 'numbers.interval',
  'units.parse', 'units.parse_bytes',
  // Objects
  'object.get', 'object.keys', 'object.union', 'object.remove', 'object.filter',
  'json.remove', 'json.patch', 'json.filter', 'json.match', 'json.marshal', 'json.unmarshal',
  // Arrays / Sets
  'array.reverse', 'array.slice', 'array.concat',
  // Type checks
  'is_number', 'is_string', 'is_boolean', 'is_array', 'is_set', 'is_object', 'is_null',
  'type_name',
  // Time
  'time.now_ns', 'time.parse_ns', 'time.parse_rfc3339_ns',
  'time.clock', 'time.date', 'time.diff',
  // Crypto
  'crypto.md5', 'crypto.sha1', 'crypto.sha256',
  'io.jwt.verify_rs256', 'io.jwt.verify_rs384', 'io.jwt.verify_rs512',
  'io.jwt.verify_es256', 'io.jwt.verify_es384', 'io.jwt.verify_es512',
  'io.jwt.verify_hs256', 'io.jwt.verify_hs384', 'io.jwt.verify_hs512',
  'io.jwt.decode', 'io.jwt.encode_sign',
  // Networking
  'net.cidr_contains', 'net.cidr_intersects', 'net.cidr_expand',
  'http.send',
  // Encoding
  'base64.encode', 'base64.decode', 'base64.url_encode', 'base64.url_decode',
  'hex.encode', 'hex.decode',
  'url.encode', 'url.decode',
  'yaml.marshal', 'yaml.unmarshal',
  'graphql.parse', 'graphql.parse_and_verify',
  // Miscellaneous
  'opa.runtime', 'print', 'trace',
  'walk', 'set',
  'rand.intn',
  'semver.compare', 'semver.is_valid',
  // Muni Town built-ins
  'xrpc_local', 'xrpc_remote',
]);

// ---------------------------------------------------------------------------
// Character helpers
// ---------------------------------------------------------------------------

function isDigit(ch: number): boolean {
  return ch >= 48 && ch <= 57;
}

function isAlpha(ch: number): boolean {
  return (ch >= 65 && ch <= 90) || (ch >= 97 && ch <= 122);
}

function isIdentStart(ch: number): boolean {
  return isAlpha(ch) || ch === 95; // a-z, A-Z, _
}

function isIdentPart(ch: number): boolean {
  return isIdentStart(ch) || isDigit(ch);
}

function isUppercase(ch: number): boolean {
  return ch >= 65 && ch <= 90;
}

// ---------------------------------------------------------------------------
// StreamLanguage tokenizer
// ---------------------------------------------------------------------------

export const regoLanguage = StreamLanguage.define<unknown>({
  name: 'rego',

  startState() {
    return {};
  },

  token(stream: any, _state: unknown) {
    // ----- Whitespace ----------------------------------------------------
    if (stream.eatSpace()) return null;

    const ch = stream.peek();

    // ----- Comments -------------------------------------------------------
    if (ch === 35 /* # */) {
      stream.next();
      const rest = stream.string.slice(stream.pos);
      if (/^\s*(METADATA|\w+:)/.test(rest)) {
        stream.skipToEnd();
        return 'meta';
      }
      stream.skipToEnd();
      return 'lineComment';
    }

    // ----- Strings: double-quoted "..." -----------------------------------
    if (ch === 34 /* " */) {
      stream.next();
      while (!stream.eol()) {
        const c = stream.next();
        if (c === 34) return 'string';
        if (c === 92) stream.next(); // skip escaped char
      }
      return 'string';
    }

    // ----- Strings: raw backtick `...` ------------------------------------
    if (ch === 96 /* ` */) {
      stream.next();
      while (!stream.eol()) {
        if (stream.next() === 96) return 'string';
      }
      return 'string';
    }

    // ----- Interpolated strings: $"..." and $`...` ------------------------
    if (ch === 36 /* $ */ && stream.pos + 1 < stream.string.length) {
      const nextCh = stream.string.charCodeAt(stream.pos + 1);
      if (nextCh === 34 /* " */ || nextCh === 96 /* ` */) {
        stream.next(); // consume $
        stream.next(); // consume opening quote
        const closeCh = nextCh === 34 ? 34 : 96;
        while (!stream.eol()) {
          const c = stream.next();
          if (c === closeCh) return 'string';
          if (c === 123 /* { */) {
            let depth = 1;
            while (depth > 0 && !stream.eol()) {
              const cc = stream.next();
              if (cc === 123) depth++;
              if (cc === 125) depth--;
            }
          }
          if (c === 92) stream.next();
        }
        return 'string';
      }
    }

    // ----- Numbers --------------------------------------------------------
    if (ch === 45 /* - */ || isDigit(ch)) {
      const start = stream.pos;
      if (stream.peek() === 45) stream.next(); // optional minus sign
      if (isDigit(stream.peek())) {
        stream.eatWhile(isDigit);
        if (stream.peek() === 46) { stream.next(); stream.eatWhile(isDigit); } // decimal part
        if (stream.peek() === 69 || stream.peek() === 101) {
          stream.next();
          if (stream.peek() === 43 || stream.peek() === 45) stream.next();
          if (isDigit(stream.peek())) stream.eatWhile(isDigit);
        }
        return 'number';
      }
      stream.pos = start;
    }

    // ----- Identifiers, keywords, builtins, variables --------------------
    if (isIdentStart(ch)) {
      const start = stream.pos;
      stream.eatWhile(isIdentPart);

      // Dotted names (e.g. data.arbiter.spaces, time.now_ns)
      while (stream.peek() === 46 && stream.pos + 1 < stream.string.length) {
        const next = stream.string.charCodeAt(stream.pos + 1);
        if (isIdentStart(next)) {
          stream.next();
          stream.eatWhile(isIdentPart);
        } else {
          break;
        }
      }

      const word = stream.string.slice(start, stream.pos);

      // 1. Rego keywords
      if (KEYWORDS.has(word)) return 'keyword';

      // 2. Boolean/nil constants
      if (CONSTANTS.has(word)) return 'atom';

      // 3. Built-in functions (count, sprintf, startswith, etc.)
      if (BUILTINS.has(word)) return 'definitionKeyword';

      // 4. Root documents (data, input) — always highlighted as definitions
      if (ROOT_DOCUMENTS.has(word)) return 'variableName.definition';

      const nextCh = stream.peek();

      // 5. Variables — start with uppercase or underscore (e.g. User, _)
      if (word.charCodeAt(0) === 95 || isUppercase(word.charCodeAt(0))) {
        return 'variableName';
      }

      // 6a. Rule definitions — foo := …
      if (nextCh === 58 && stream.pos + 1 < stream.string.length &&
          stream.string.charCodeAt(stream.pos + 1) === 61) {
        return 'variableName.function.definition';
      }

      // 6b. Rule definitions — foo[ … ]  or  foo { … }
      if (nextCh === 91 /* [ */ || nextCh === 123 /* { */) {
        return 'variableName.function.definition';
      }

      // 7. Function calls — foo( … )
      if (nextCh === 40 /* ( */) {
        return 'variableName.function';
      }

      // 8. Plain identifier (rule ref, field name) — style as variable
      return 'variableName';
    }

    // ----- Operators ------------------------------------------------------
    if (ch === 61) {
      stream.next();
      if (stream.peek() === 61) stream.next();
      return 'operator';
    }
    if (ch === 33) {
      stream.next();
      if (stream.peek() === 61) stream.next();
      return 'operator';
    }
    if (ch === 62 || ch === 60) {
      stream.next();
      if (stream.peek() === 61) stream.next();
      return 'operator';
    }
    if (ch === 43 || ch === 45 || ch === 42 || ch === 37 || ch === 47) {
      stream.next();
      return 'operator';
    }
    if (ch === 124 || ch === 38) {
      stream.next();
      return 'operator';
    }
    if (ch === 58) {
      stream.next();
      if (stream.peek() === 61) stream.next();
      return 'operator';
    }

    // ----- Punctuation ----------------------------------------------------
    if ('()[]{}.,;'.includes(String.fromCharCode(ch))) {
      stream.next();
      return 'punctuation';
    }

    stream.next();
    return null;
  },

  languageData: {
    commentTokens: { line: '#' },
    closeBrackets: { brackets: ['(', '[', '{', '"'] },
  },
});

