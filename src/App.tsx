import { Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import Timer from "./routes/Timer";
import Rules from "./routes/Rules";
import Schedules from "./routes/Schedules";
import Stats from "./routes/Stats";
import Settings from "./routes/Settings";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Timer />} />
        <Route path="/rules" element={<Rules />} />
        <Route path="/schedules" element={<Schedules />} />
        <Route path="/stats" element={<Stats />} />
        <Route path="/settings" element={<Settings />} />
      </Route>
    </Routes>
  );
}