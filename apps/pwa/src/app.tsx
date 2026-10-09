import { LocationProvider, Route, Router } from 'preact-iso';
import { Home } from './pages/home';
import { NotFound } from './pages/not-found';

export function App() {
  return (
    <LocationProvider>
      <Router>
        <Route path="/" component={Home} />
        <Route default component={NotFound} />
      </Router>
    </LocationProvider>
  );
}
