import globals from "globals";
import tseslint from "typescript-eslint";

import strict from "./eslint.strict.mjs";

export default tseslint.config(
  ...strict,
  {
    ignores: [
      "target/**",
      "registry/target/**",
      "registry/.wrangler/**",
      "registry/auth/.wrangler/**",
      "review/dependency-sources/ejm-ui/**",
      "sdk/typescript/src/protocol.ts",
      // Vendored copy of the shared design kit; edit it upstream.
      "site/public/design-kit/**",
    ],
  },
  { files: ["**/*.mjs"], languageOptions: { globals: globals.node } },
  { files: ["registry/public/**/*.js"], languageOptions: { globals: globals.browser } },
  {
    files: ["registry/tests/fixtures/**/*.js"],
    languageOptions: { globals: globals.serviceworker },
  },
  {
    files: ["**/*.ts"],
    rules: {
      "@typescript-eslint/explicit-function-return-type": "error",
      "@typescript-eslint/no-floating-promises": ["error", { ignoreVoid: false }],
    },
  },
);
