import { Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import Timer from "./routes/Timer";
import Rules from "./routes/Rules";
import Quotas from "./routes/Quotas";
import Stats from "./routes/Stats";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Timer />} />
        <Route path="/rules" element={<Rules />} />
        <Route path="/quotas" element={<Quotas />} />
        <Route path="/stats" element={<Stats />} />
      </Route>
    </Routes>
  );
}