import { Component, type ReactNode } from 'react';
import { t, type Lang } from '../lib/i18n';
import { BackupDownload } from './BackupDownload';

export class PageErrorBoundary extends Component<{
  lang: Lang;
  exportWorkspace: () => string;
  children: ReactNode;
}, { error: string | null }> {
  state: { error: string | null } = { error: null };

  static getDerivedStateFromError(error: unknown) {
    return { error: error instanceof Error ? error.message : String(error) };
  }

  render() {
    if (this.state.error === null) return this.props.children;
    const { lang, exportWorkspace } = this.props;
    return <section className="shell-alert" role="alert">
      <h2>{t(lang, 'shell.pageError')}</h2>
      <p>{t(lang, 'shell.pageErrorBackup')}</p>
      <pre>{this.state.error}</pre>
      <BackupDownload lang={lang} exportWorkspace={exportWorkspace} />
    </section>;
  }
}
