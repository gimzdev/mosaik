import { getCurrentWindow } from '@tauri-apps/api/window';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { Settings } from './settings/Settings';
import { SwitcherWindow } from './switcher/SwitcherWindow';
import './styles.css';

const root = document.getElementById('root');
if (!root) throw new Error('index.html has no #root element');

// One bundle for every window; each window picks its view by label.
const label = getCurrentWindow().label;
createRoot(root).render(<StrictMode>{label === 'switcher' ? <SwitcherWindow /> : <Settings />}</StrictMode>);
