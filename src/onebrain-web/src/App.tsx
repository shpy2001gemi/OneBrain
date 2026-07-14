import { useState } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { AppShell } from './components/AppShell';
import { AuthGate } from './components/AuthGate';
import { SetupWizard } from './components/SetupWizard';
import { DashboardPage } from './pages/Dashboard';
import { ExplorerPage } from './pages/Explorer';
import { EncodePage } from './pages/Encode';
import { ChatPage } from './pages/Chat';
import { GraphPage } from './pages/Graph';
import { PomvPage } from './pages/Pomv';
import { NetworkPage } from './pages/NetworkPage';
import { WalletPage } from './pages/Wallet';
import { SettingsPage } from './pages/Settings';
import { DataToolsPage } from './pages/DataTools';
import { SocialPage } from './pages/Social';
import { DevicesPage } from './pages/Devices';
import { DiscoveryPage } from './pages/Discovery';
import { CollectionsPage } from './pages/Collections';
import { AnalyticsPage } from './pages/Analytics';
import { HelpPage } from './pages/Help';

export default function App() {
  const [setupDone, setSetupDone] = useState(
    () => localStorage.getItem('ob_setup_complete') === 'true'
  );

  return (
    <BrowserRouter>
      <AuthGate>
        {!setupDone && (
          <SetupWizard onComplete={() => setSetupDone(true)} />
        )}
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
            <Route path="/data-tools" element={<DataToolsPage />} />
            <Route path="/social" element={<SocialPage />} />
            <Route path="/devices" element={<DevicesPage />} />
            <Route path="/discovery" element={<DiscoveryPage />} />
            <Route path="/collections" element={<CollectionsPage />} />
            <Route path="/analytics" element={<AnalyticsPage />} />
            <Route path="/help" element={<HelpPage />} />
          </Route>
        </Routes>
      </AuthGate>
    </BrowserRouter>
  );
}
