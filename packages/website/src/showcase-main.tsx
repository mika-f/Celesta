import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { Showcase } from './Showcase';
import './style.css';

createRoot(document.getElementById('root')!).render(<StrictMode><Showcase /></StrictMode>);
