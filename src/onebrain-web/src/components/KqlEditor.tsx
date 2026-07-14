import { useEffect, useRef, useCallback } from 'react';
import { EditorView, keymap, placeholder as cmPlaceholder } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { autocompletion, CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { searchKeymap } from '@codemirror/search';
import { oneDark } from '@codemirror/theme-one-dark';

// KQL keywords for autocompletion
const KQL_KEYWORDS = [
  'SELECT', 'FROM', 'WHERE', 'AND', 'OR', 'NOT', 'LIKE', 'IN', 'BETWEEN',
  'ORDER', 'BY', 'ASC', 'DESC', 'LIMIT', 'OFFSET', 'COUNT', 'SUM', 'AVG',
  'GROUP', 'HAVING', 'JOIN', 'ON', 'AS', 'DISTINCT', 'EXISTS', 'NULL',
  'TRUE', 'FALSE', 'IS',
];

const KQL_FIELDS = [
  'gene_type', 'content', 'pomv', 'trust', 'created', 'wire_size',
  'confidence', 'epistemic', 'evidence', 'verification_status',
  'instruction_count', 'outgoing_bond_count', 'incoming_bond_count',
  'cid_hex', 'codons', 'bonds',
];

const KQL_GENE_TYPES = [
  'Fact', 'Procedure', 'Experience', 'Creative', 'MediaExperience',
  'Testimony', 'Formal', 'Hypothesis', 'Narrative', 'Sensory',
  'Composite', 'Normative', 'Definition',
];

function kqlCompletion(context: CompletionContext): CompletionResult | null {
  const word = context.matchBefore(/\w*/);
  if (!word || (word.from === word.to && !context.explicit)) return null;

  const options = [
    ...KQL_KEYWORDS.map(k => ({ label: k, type: 'keyword' as const, boost: 2 })),
    ...KQL_FIELDS.map(f => ({ label: f, type: 'property' as const, boost: 1 })),
    ...KQL_GENE_TYPES.map(g => ({ label: `'${g}'`, type: 'enum' as const, detail: 'GeneType' })),
  ];

  return { from: word.from, options };
}

interface KqlEditorProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit?: () => void;
  placeholder?: string;
  minHeight?: number;
}

export function KqlEditor({ value, onChange, onSubmit, placeholder = 'Enter KQL query...', minHeight = 120 }: KqlEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onSubmitRef = useRef(onSubmit);

  // Keep refs up to date
  onChangeRef.current = onChange;
  onSubmitRef.current = onSubmit;

  const handleSubmit = useCallback(() => {
    onSubmitRef.current?.();
    return true;
  }, []);

  useEffect(() => {
    if (!containerRef.current) return;

    const state = EditorState.create({
      doc: value,
      extensions: [
        history(),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...searchKeymap,
          { key: 'Ctrl-Enter', run: () => handleSubmit() },
          { key: 'Mod-Enter', run: () => handleSubmit() },
        ]),
        autocompletion({ override: [kqlCompletion] }),
        cmPlaceholder(placeholder),
        oneDark,
        EditorView.updateListener.of(update => {
          if (update.docChanged) {
            onChangeRef.current(update.state.doc.toString());
          }
        }),
        EditorView.theme({
          '&': {
            minHeight: `${minHeight}px`,
            border: '1px solid var(--ob-glass-border)',
            borderRadius: 'var(--ob-radius-md)',
            overflow: 'hidden',
          },
          '.cm-scroller': {
            fontFamily: 'var(--ob-font-mono, "JetBrains Mono", monospace)',
            fontSize: '0.88rem',
          },
          '.cm-focused': {
            outline: 'none',
            borderColor: 'var(--ob-accent)',
          },
        }),
      ],
    });

    const view = new EditorView({
      state,
      parent: containerRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, []); // Only run once on mount

  // Sync external value changes
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const currentDoc = view.state.doc.toString();
    if (currentDoc !== value) {
      view.dispatch({
        changes: { from: 0, to: currentDoc.length, insert: value },
      });
    }
  }, [value]);

  return <div ref={containerRef} />;
}
