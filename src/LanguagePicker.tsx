import {Globe} from 'lucide-react';
import {setLanguage,t,useLanguage,type Language} from './i18n';
export default function LanguagePicker(){
 const language=useLanguage();
 return <label className="language-picker"><Globe size={14} aria-hidden="true"/><select aria-label={t('Idioma')} title={t('Idioma')} value={language} onChange={event=>setLanguage(event.target.value as Language)}><option value="pt-BR">Português (BR)</option><option value="en">English</option><option value="es">Español</option></select></label>
}
