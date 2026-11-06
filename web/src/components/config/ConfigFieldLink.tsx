import type { ReactNode } from 'react';
import { useNavigateToPage } from '../../app/pages/navigate';
import { bindT, type Lang } from '../../lib/i18n';
import { requestConfigField } from '../../lib/configNavigation';

export function ConfigFieldLink({ field, lang, children }: { field: string; lang: Lang; children?: ReactNode }) {
  const navigate = useNavigateToPage();
  return <a className="config-field-link" href={`/config#config-${encodeURIComponent(field)}`} onClick={event => {
    event.preventDefault();
    navigate('config');
    requestConfigField(field);
  }}>{children ?? bindT(lang)('calcs.enableConfig')}</a>;
}
