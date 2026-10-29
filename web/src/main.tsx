import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './app/App';
import './styles/global.css';
import './app/styles.css';
import './app/feature-layout.css';
import './app/pages.css';
import './app/pages-skills.css';
import './app/pages-config.css';
import './app/pages-trade.css';
import './app/pages-compare.css';
import './app/pages-calcs.css';
import './app/pages-tree.css';
import './app/inspector.css';
import './app/display-modes.css';

createRoot(document.getElementById('root')!).render(<StrictMode><App /></StrictMode>);
