import { LocationProvider, Route, Router, useLocation } from 'preact-iso';
import { Layout } from './components/Layout';
import { CallPage } from './pages/CallPage';
import { ChatPage } from './pages/ChatPage';
import { Home } from './pages/Home';
import { NeutralPage } from './pages/NeutralPage';
import { NotFound } from './pages/NotFound';
import { PrivacyPage } from './pages/PrivacyPage';
import { SettingsPage } from './pages/SettingsPage';
import { SilentPage } from './pages/SilentPage';
import { TranslationPage } from './pages/TranslationPage';

function Shell() {
  const { path } = useLocation();
  // La pantalla neutral no muestra nada del programa.
  return (
    <Layout minimal={path === '/clima'}>
      <Router>
        <Route path="/" component={Home} />
        <Route path="/chat" component={ChatPage} />
        <Route path="/silencioso" component={SilentPage} />
        <Route path="/llamada" component={CallPage} />
        <Route path="/idioma" component={TranslationPage} />
        <Route path="/ajustes" component={SettingsPage} />
        <Route path="/aviso-de-privacidad" component={PrivacyPage} />
        <Route path="/clima" component={NeutralPage} />
        <Route default component={NotFound} />
      </Router>
    </Layout>
  );
}

export function App() {
  return (
    <LocationProvider>
      <Shell />
    </LocationProvider>
  );
}
