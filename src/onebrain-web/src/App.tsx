import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { AppShell } from './components/AppShell';
import { AuthGate } from './components/AuthGate';
import { DashboardPage } from './pages/Dashboard';
import { ExplorerPage } from './pages/Explorer';
import { EncodePage } from './pages/Encode';
import { ChatPage } from './pages/Chat';
import { GraphPage } from './pages/Graph';
import { PomvPage } from './pages/Pomv';
import { NetworkPage } from './pages/NetworkPage';
import { WalletPage } from './pages/Wallet';
import { SettingsPage } from './pages/Settings';

export default function App() {
  return (
    <BrowserRouter>
      <AuthGate>
        <Routes>
          <Route element={<AppShell />}>
            <Route path="/" element={<DashboardPage />} />
            <Route path="/explorer" element={<ExplorerPage />} />
            <Route path="/encode" element={<EncodePage />} />
            <Route path="/chat" element={<ChatPage />} />
            <Route path="/graph" element={<GraphPage />} />
            <Route path="/pomv" element={<PomvPage />} />
            <Route path="/network" element={<NetworkPage />} />
            <Route path="/wallet" element={<WalletPage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Route>
        </Routes>
      </AuthGate>
    </BrowserRouter>
  );
}
