'use strict';

// These are never invoked as functions. `render.js` walks the JSX element
// tree and matches host elements by reference against this module's exports,
// so they only need to exist as stable, unique identities.

function Composition() {
  return null;
}

function Group() {
  return null;
}

function Image() {
  return null;
}

function Text() {
  return null;
}

module.exports = { Composition, Group, Image, Text };
