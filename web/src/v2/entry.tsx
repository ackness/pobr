import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import '../styles/global.css';
import './styles.css';
import './feature-layout.css';
import './pages.css';
import './pages-skills.css';
import './pages-config.css';
import './pages-trade.css';
import './pages-compare.css';
import './pages-calcs.css';
import './pages-tree.css';
import './inspector.css';

createRoot(document.getElementById('root')!).render(<StrictMode><App /></StrictMode>);
