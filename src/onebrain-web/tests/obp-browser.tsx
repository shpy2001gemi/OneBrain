// Isolated visual fixture. No live API, host, model or networking mutation.
import { createRoot } from 'react-dom/client';
import { NetworkPage } from '../src/pages/NetworkPage';
import { fixture } from './obpFixtures';
import '../src/index.css';
const f = fixture();
createRoot(document.getElementById('root')!).render(<NetworkPage client={f.client} />);
