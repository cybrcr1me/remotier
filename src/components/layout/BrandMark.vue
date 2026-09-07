<script setup lang="ts">
/**
 * The Remotier mark: a lime tile carrying a black `>`.
 *
 * The glyph is the outlined Martian Mono 700 `>` from `design/mark.svg` - a `<text>`
 * element would fall back to some other monospace wherever the face is missing. The full
 * mark is `>_`, but the underscore is dropped at this size for the same reason the 16px
 * app icon drops it: below about 24px the two shapes stop reading as one mark.
 *
 * Regenerate the source artwork with `bun run scripts/outline-brand-svg.js`.
 */
</script>

<template>
  <svg viewBox="0 0 32 32" aria-hidden="true" focusable="false">
    <rect width="32" height="32" fill="var(--rm-lime)" />
    <g fill="var(--rm-on-lime)" transform="translate(9.7 23.3) scale(0.018 -0.018)">
      <path d="M88 22L88 176L515 341L515 359L88 524L88 678L612 470L612 230Z" />
    </g>
  </svg>
</template>
