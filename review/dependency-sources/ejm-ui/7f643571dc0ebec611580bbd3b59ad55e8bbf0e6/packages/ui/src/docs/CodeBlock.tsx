import { CodeBlockFrame, type CodeBlockFrameProps } from './CodeBlockFrame';
import { CodeHighlight } from './CodeHighlight';

export type CodeBlockProps = Omit<CodeBlockFrameProps, 'children'>;

/** A complete highlighted code figure with the shared copy control. */
export function CodeBlock(props: CodeBlockProps) {
  return <CodeBlockFrame {...props}><CodeHighlight code={props.code} lang={props.lang} label={props.label} /></CodeBlockFrame>;
}
