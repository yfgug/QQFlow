import { HashRouter } from 'react-router-dom'
import { createRoot } from 'react-dom/client'
import App from './App'
import { AppProvider } from './stores/appStore'
import { ThemeProvider } from './stores/themeStore'
import './styles/main.scss'

createRoot(document.getElementById('app')!).render(
  <HashRouter>
    <ThemeProvider>
      <AppProvider>
        <App />
      </AppProvider>
    </ThemeProvider>
  </HashRouter>
)
