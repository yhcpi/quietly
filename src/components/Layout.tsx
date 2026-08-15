import { NavLink, Outlet } from "react-router-dom";
import "./Layout.css";

const NAV = [
  { to: "/", label: "Timer", end: true },
  { to: "/rules", label: "Rules" },
  { to: "/schedules", label: "Schedules" },
  { to: "/stats", label: "Stats" },
  { to: "/settings", label: "Settings" },
];

export default function Layout() {
  return (
    <div className="app-shell">
      <header className="app-header">
        <span className="app-brand">Quietly</span>
        <nav className="app-nav">
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className={({ isActive }) =>
                "app-nav-link" + (isActive ? " app-nav-link-active" : "")
              }
            >
              {item.label}
            </NavLink>
          ))}
        </nav>
      </header>
      <main className="app-main">
        <Outlet />
      </main>
    </div>
  );
}