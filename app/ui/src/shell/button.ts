import { cva } from 'class-variance-authority';

// The button kinds from docs/design.md, "Buttons". Secondary is the default.
export const button = cva('button', {
  variants: {
    kind: {
      secondary: null,
      primary: 'button-primary',
      danger: 'button-quiet-danger',
      icon: 'button-icon',
    },
    size: {
      regular: null,
      small: 'button-small',
    },
  },
  defaultVariants: { kind: 'secondary', size: 'regular' },
});
