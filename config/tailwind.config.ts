/*
 * Name: tailwind.config.ts
 * Purpose: Tailwind configuration with custom design tokens mapped from CSS
 *   variables.
 * Description: All colors reference CSS custom properties for theme
 *   switching. Loaded by Tailwind 4 through the @config directive in
 *   globals.css rather than being discovered automatically, which is
 *   how v4 supports a JavaScript config. Font stack: Play (display/UI), Source Serif 4
 *   (body/editor), JetBrains Mono (code). 8-point spacing grid
 *   enforced via default spacing scale.
 * Tech Stack: Tailwind CSS 4
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

import type { Config } from "tailwindcss";


export default {
  content: ["./src/index.html", "./src/**/*.{ts,tsx}"],

  theme: {
    /* Top-level overrides: replace Tailwind's default font stacks entirely.
       This ensures Play is the base UI font everywhere, not system-ui. */
    fontFamily: {
      sans: ['"Play"', "sans-serif"],
      display: ['"Play"', "sans-serif"],
      body: ['"Source Serif 4"', "serif"],
      serif: ['"Source Serif 4"', "serif"],
      mono: ['"JetBrains Mono"', "monospace"],
    },

    extend: {

      colors: {
        bg: "var(--color-bg)",
        surface: "var(--color-surface)",
        "surface-2": "var(--color-surface-2)",
        "surface-3": "var(--color-surface-3)",
        border: "var(--color-border)",
        "border-hover": "var(--color-border-hover)",
        "text-1": "var(--color-text-1)",
        "text-2": "var(--color-text-2)",
        "text-3": "var(--color-text-3)",
        "text-4": "var(--color-text-4)",
        accent: "var(--color-accent)",
        "accent-dim": "var(--color-accent-dim)",
        primary: "var(--color-primary)",
        "primary-hover": "var(--color-primary-hover)",
        "on-primary": "var(--color-on-primary)",
        mark: "var(--color-mark)",
        error: "var(--color-error)",
      },

      fontSize: {
        /* Strict typographic scale in rem so OS and browser text-size
           preferences scale the whole interface. 2xs (11px at default size)
           is the legibility floor. */
        "2xs": ["0.6875rem", { lineHeight: "1rem" }],
        xs: ["0.75rem", { lineHeight: "1rem" }],
        sm: ["0.875rem", { lineHeight: "1.25rem" }],
        base: ["1rem", { lineHeight: "1.5rem" }],
        lg: ["1.125rem", { lineHeight: "1.75rem" }],
        xl: ["1.375rem", { lineHeight: "1.875rem" }],
        "2xl": ["1.75rem", { lineHeight: "2.25rem" }],
        "3xl": ["2.25rem", { lineHeight: "2.75rem" }],
      },

      borderColor: {
        DEFAULT: "var(--color-border)",
      },

      /* Motion primitives. Deliberately short and soft; the global
         prefers-reduced-motion block in globals.css neutralizes them, and
         staggered entrances are gated behind motion-safe: at the call site. */
      keyframes: {
        "fade-in": {
          from: { opacity: "0" },
          to: { opacity: "1" },
        },
        "scale-in": {
          from: { opacity: "0", transform: "scale(0.97)" },
          to: { opacity: "1", transform: "scale(1)" },
        },
        "slide-up": {
          from: { opacity: "0", transform: "translateY(8px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
      },
      animation: {
        "fade-in": "fade-in 150ms ease-out",
        "scale-in": "scale-in 170ms cubic-bezier(0.16, 1, 0.3, 1)",
        "slide-up": "slide-up 240ms cubic-bezier(0.16, 1, 0.3, 1) both",
      },
    },
  },

  plugins: [],
} satisfies Config;
