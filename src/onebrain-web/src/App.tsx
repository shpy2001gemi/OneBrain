import { Suspense, lazy, useState } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { AppShell } from './components/AppShell';
import { AuthGate } from './components/AuthGate';
import { ErrorBoundary } from './components/ErrorBoundary';
import { SetupWizard } from './components/SetupWizard';

const DashboardPage = lazy(() => import('./pages/Dashboard').then(m => ({ default: m.DashboardPage })));
const ExplorerPage = lazy(() => import('./pages/Explorer').then(m => ({ default: m.ExplorerPage })));
const EncodePage = lazy(() => import('./pages/Encode').then(m => ({ default: m.EncodePage })));
const ChatPage = lazy(() => import('./pages/Chat').then(m => ({ default: m.ChatPage })));
const GraphPage = lazy(() => import('./pages/Graph').then(m => ({ default: m.GraphPage })));
const PomvPage = lazy(() => import('./pages/Pomv').then(m => ({ default: m.PomvPage })));
const NetworkPage = lazy(() => import('./pages/NetworkPage').then(m => ({ default: m.NetworkPage })));
const WalletPage = lazy(() => import('./pages/Wallet').then(m => ({ default: m.WalletPage })));
const SettingsPage = lazy(() => import('./pages/Settings').then(m => ({ default: m.SettingsPage })));
const DataToolsPage = lazy(() => import('./pages/DataTools').then(m => ({ default: m.DataToolsPage })));
const SocialPage = lazy(() => import('./pages/Social').then(m => ({ default: m.SocialPage })));
const DevicesPage = lazy(() => import('./pages/Devices').then(m => ({ default: m.DevicesPage })));
const DiscoveryPage = lazy(() => import('./pages/Discovery').then(m => ({ default: m.DiscoveryPage })));
const CollectionsPage = lazy(() => import('./pages/Collections').then(m => ({ default: m.CollectionsPage })));
const AnalyticsPage = lazy(() => import('./pages/Analytics').then(m => ({ default: m.AnalyticsPage })));
const DraftsPage = lazy(() => import('./pages/Drafts').then(m => ({ default: m.DraftsPage })));
const FilesPage = lazy(() => import('./pages/Files').then(m => ({ default: m.FilesPage })));
const HelpPage = lazy(() => import('./pages/Help').then(m => ({ default: m.HelpPage })));

function LoadingFallback() {
  return (
    <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh' }}>
      <div className="spinner spinner-lg" />
    </div>
  );
}

export default function App() {
  const [setupDone, setSetupDone] = useState(
    () => localStorage.getItem('ob_setup_complete') === 'true'
  );

  return (
    <ErrorBoundary>
    <BrowserRouter>
      <AuthGate>
        {setupDone ? (
          <Suspense fallback={<LoadingFallback />}>
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
                <Route path="/drafts" element={<DraftsPage />} />
                <Route path="/files" element={<FilesPage />} />
                <Route path="/help" element={<HelpPage />} />
                <Route path="*" element={
                  <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', minHeight: '60vh' }}>
                    <div style={{ fontSize: 64, marginBottom: 16 }}>🔍</div>
                    <h1 style={{ fontSize: '1.5rem', marginBottom: 8 }}>Page Not Found</h1>
                    <p style={{ color: 'var(--ob-text-secondary)' }}>The page you're looking for doesn't exist.</p>
                  </div>
                } />
              </Route>
            </Routes>
          </Suspense>
        ) : (
          <SetupWizard onComplete={() => setSetupDone(true)} />
        )}
      </AuthGate>
    </BrowserRouter>
    </ErrorBoundary>
  );
}
