export function languageName(lang: string): string {
  const names: Record<string, string> = { ts: 'TypeScript', tsx: 'TSX', js: 'JavaScript', jsx: 'JSX', css: 'CSS', html: 'HTML', json: 'JSON', rust: 'Rust', rs: 'Rust', py: 'Python', python: 'Python', ps1: 'PowerShell', powershell: 'PowerShell', pwsh: 'PowerShell', toml: 'TOML', sh: 'Shell', bash: 'Shell', shell: 'Shell', text: 'Text' };
  return names[lang] ?? lang;
}
