"use client";

import { useCallback, useEffect, useState } from "react";
import { CONTRACTS, DEMO_ACCOUNT, EXPLORER, RELAYER_URL, ROSTER } from "../lib/config";
import { getInventory, getFtBalance, getRate, fmtToken } from "../lib/near";
import { fetchActivity, buildFeed, type FeedEntry } from "../lib/indexer";
import { useWallet } from "../lib/wallet";

interface Live {
  inventory: Record<string, string>;
  kzr: string;
  nxc: string;
  rate: [string, string];
}

const OBJECTIVES = [
  { label: "Deploy to the Nexus Zone", state: "done" },
  { label: "Reach the power node", state: "done" },
  { label: "Hold the node", state: "active" },
  { label: "Stabilize & awaken", state: "" },
];

export default function Home() {
  const { accountId, signIn, signOut, signAndSend } = useWallet();
  const account = accountId ?? DEMO_ACCOUNT;
  const [live, setLive] = useState<Live | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [nxc, setNxc] = useState("");
  const [busy, setBusy] = useState(false);
  const [claiming, setClaiming] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [feed, setFeed] = useState<FeedEntry[] | null>(null);
  const [feedDown, setFeedDown] = useState(false);

  const load = useCallback(async () => {
    try {
      setErr(null);
      const ids = ROSTER.map((r) => r.token_id);
      const [bals, kzr, nxcBal, rate] = await Promise.all([
        getInventory(account, ids),
        getFtBalance(CONTRACTS.token, account).catch(() => "0"),
        getFtBalance(CONTRACTS.coin, account).catch(() => "0"),
        getRate(),
      ]);
      const inventory: Record<string, string> = {};
      ROSTER.forEach((r, i) => (inventory[r.token_id] = bals[i] ?? "0"));
      setLive({ inventory, kzr, nxc: nxcBal, rate });
    } catch (e) {
      setErr(String(e));
    }
  }, [account]);

  const loadFeed = useCallback(async () => {
    try {
      setFeedDown(false);
      setFeed(buildFeed(await fetchActivity(account)));
    } catch {
      setFeedDown(true);
      setFeed([]);
    }
  }, [account]);

  useEffect(() => {
    setLive(null);
    setFeed(null);
    load();
    loadFeed();
  }, [load, loadFeed]);

  useEffect(() => {
    const t = setInterval(() => {
      load();
      loadFeed();
    }, 25000);
    return () => clearInterval(t);
  }, [load, loadFeed]);

  const convert = useCallback(async () => {
    if (!accountId) return signIn();
    const whole = Math.floor(Number(nxc));
    if (!whole || whole <= 0) return setNote("Enter an NXC amount to convert.");
    setBusy(true);
    setNote(null);
    try {
      const yocto = (BigInt(whole) * 10n ** 18n).toString();
      await signAndSend(
        CONTRACTS.coin,
        "ft_transfer_call",
        { receiver_id: CONTRACTS.convert, amount: yocto, memo: null, msg: "" },
        120,
        "1",
      );
      setNote("Conversion submitted — balances & activity update shortly.");
      load();
      setTimeout(loadFeed, 7000);
      setTimeout(loadFeed, 20000);
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  }, [accountId, nxc, signIn, signAndSend, load, loadFeed]);

  const claim = useCallback(async () => {
    if (!accountId) return signIn();
    setClaiming(true);
    setNote(null);
    try {
      const r = await fetch(`${RELAYER_URL}/relay/claim`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ account_id: accountId, mission_id: "awaken-the-nexus" }),
      });
      const j = await r.json();
      if (!r.ok || j.error) throw new Error(j.error ?? "relay failed");
      setNote("Loot claimed — gasless. Inventory & activity updating…");
      setTimeout(load, 1500);
      setTimeout(loadFeed, 7000);
      setTimeout(loadFeed, 20000);
    } catch (e) {
      setNote(String(e));
    } finally {
      setClaiming(false);
    }
  }, [accountId, signIn, load, loadFeed]);

  const rateLabel = live ? `${live.rate[1]} NXC = ${live.rate[0]} KZR` : "…";

  return (
    <>
      <svg width="0" height="0" style={{ position: "absolute" }} aria-hidden="true">
        <defs>
          <symbol id="k" viewBox="0 0 24 24"><path d="M5 2h5v7l6-7h6l-8 9 8 11h-6l-6-8v8H5z" fill="#57e02a" /></symbol>
          <symbol id="i-ammo" viewBox="0 0 24 24"><rect x="9" y="3" width="6" height="14" rx="3" fill="#f0a63c" /><rect x="8" y="17" width="8" height="4" rx="1" fill="#f0a63c" opacity=".6" /></symbol>
          <symbol id="i-med" viewBox="0 0 24 24"><rect x="3" y="7" width="18" height="12" rx="3" fill="none" stroke="#63d67f" strokeWidth="2" /><path d="M12 10v6M9 13h6" stroke="#63d67f" strokeWidth="2" /></symbol>
          <symbol id="i-mod" viewBox="0 0 24 24"><path d="M12 2l8 5v10l-8 5-8-5V7z" fill="none" stroke="#4aa0f0" strokeWidth="2" /><circle cx="12" cy="12" r="3" fill="#4aa0f0" /></symbol>
          <symbol id="i-skin" viewBox="0 0 24 24"><path d="M12 3l7 3v6c0 5-7 9-7 9s-7-4-7-9V6z" fill="none" stroke="#c56bff" strokeWidth="2" /></symbol>
          <symbol id="i-ach" viewBox="0 0 24 24"><circle cx="12" cy="9" r="6" fill="none" stroke="#f7d64a" strokeWidth="2" /><path d="M9 14l-2 7 5-3 5 3-2-7" fill="none" stroke="#f7d64a" strokeWidth="2" /></symbol>
          <symbol id="i-wpn" viewBox="0 0 24 24"><path d="M3 12h13l5-4v8l-5-4M7 12v5" stroke="#ff6b5c" strokeWidth="2" fill="none" /></symbol>
        </defs>
      </svg>

      <div className="wrap">
        <div className="topbar">
          <div className="mark"><svg><use href="#k" /></svg></div>
          <div className="brand"><b>Squad&nbsp;Legacy</b><span>Kruzer Ultraverse</span></div>
          <div className="spacer"></div>
          <div className="bal">
            <span className="chip"><span className="k">KZR</span><span className="v">{live ? fmtToken(live.kzr) : "…"}</span></span>
            <span className="chip"><span className="k">NXC</span><span className="v">{live ? fmtToken(live.nxc, 18, 0) : "…"}</span></span>
          </div>
          {accountId ? (
            <button className="wallet" onClick={signOut}><span className="dot"></span>{accountId} ✕</button>
          ) : (
            <button className="wallet" onClick={signIn}>Connect Wallet</button>
          )}
        </div>

        {!accountId && (
          <div className="notice" style={{ marginTop: 12 }}>
            Viewing <b>{DEMO_ACCOUNT}</b> (live). Connect a testnet wallet to play as your Pioneer.
          </div>
        )}

        <div className="hero">
          <div className="panel mission">
            <div className="eyebrow">Active Mission · Nexus Zone 07</div>
            <h2>Awaken the Nexus</h2>
            <div className="sephrenia">
              <div>
                <div className="who">◈ Sephrenia</div>
                <p>Pioneer — the dormant power node is within reach. Reclaim it, and a fragment of the fractured universe returns to the Restoration.</p>
              </div>
            </div>
            <ul className="objectives">
              {OBJECTIVES.map((o, i) => (
                <li key={i} className={o.state}>
                  <span className="onode">{o.state === "done" ? "✓" : i + 1}</span>
                  <span className="olabel">{o.label}</span>
                  <span className="ostat">{o.state === "done" ? "Complete" : o.state === "active" ? "In progress" : "Locked"}</span>
                </li>
              ))}
            </ul>
            <button type="button" className="btn btn-primary btn-block" onClick={claim} disabled={claiming}>
              {accountId ? (claiming ? "Claiming…" : "Claim Loot") : "Connect to Claim"} <span className="gasless">Gasless</span>
            </button>
          </div>

          <div className="sidecol">
            <div className="panel convert">
              <div className="card-h"><h3>Convert</h3><span className="rule"></span></div>
              <div className="row">
                <input
                  className="tok"
                  inputMode="numeric"
                  placeholder="NXC amount"
                  value={nxc}
                  onChange={(e) => setNxc(e.target.value.replace(/[^0-9]/g, ""))}
                  style={{ font: "700 18px/1.2 var(--mono)", color: "var(--text)" }}
                  aria-label="NXC amount to convert"
                />
                <div className="arrow">→</div>
                <div className="tok"><div className="t">KZR</div><div className="a">{nxc ? (Number(nxc) / Number(live?.rate?.[1] ?? 100)).toFixed(2) : "0.00"}</div></div>
              </div>
              <div className="rate">Rate <b>{rateLabel}</b> · <span className="oneway">one-way, irreversible</span></div>
              <button className="btn btn-primary btn-block" onClick={convert} disabled={busy}>
                {accountId ? (busy ? "Submitting…" : "Convert to KZR") : "Connect to Convert"}
              </button>
              {note && <div className="notice">{note}</div>}
            </div>

            <div className="panel">
              <div className="card-h"><h3>Craft</h3><span className="rule"></span></div>
              <div className="recipe">
                <span className="ing"><span className="n">20×</span> Rifle Cell</span>
                <span className="plus">+</span>
                <span className="ing"><span className="n">2×</span> Weapon-Mod Fragment</span>
                <span className="eq">=</span>
                <span className="out">MK-1 Stability Module</span>
              </div>
              <div style={{ height: 14 }}></div>
              <button type="button" className="btn btn-ghost btn-block" disabled title="Gasless craft — coming soon">
                Craft Upgrade <span className="gasless">Gasless · soon</span>
              </button>
            </div>
          </div>
        </div>

        <div className="section">
          <div className="sec-head"><h2>Inventory</h2><a className="muted mono" href={`${EXPLORER}/${CONTRACTS.assets}`} target="_blank" rel="noreferrer">{CONTRACTS.assets} ↗</a></div>
          {err && <div className="notice">Could not reach RPC: {err}</div>}
          <div className="grid">
            {ROSTER.map((it) => {
              const qty = live ? live.inventory[it.token_id] ?? "0" : "…";
              const empty = live && qty === "0";
              return (
                <div key={it.token_id} className={`item${empty ? " empty" : ""}`}>
                  <div className="art"><svg><use href={`#${it.icon}`} /></svg></div>
                  <div className="nm">{it.name}</div>
                  <div className="tid">{it.token_id}</div>
                  <div className="foot"><span className="qty">{qty}</span><span className="cat" style={{ color: it.color }}>{it.category}</span></div>
                </div>
              );
            })}
          </div>
        </div>

        <div className="section">
          <div className="sec-head"><h2>Activity</h2><span className="muted mono">NEP-297 event feed · indexed live</span></div>
          <div className="panel feed">
            {feed === null && <div className="ev"><span className="muted">Loading activity…</span></div>}
            {feed !== null && feedDown && (
              <div className="ev"><span className="ico" style={{ color: "var(--warn)" }}>!</span><span className="muted">Activity feed temporarily unavailable — the on-chain reads above are live.</span></div>
            )}
            {feed !== null && !feedDown && feed.length === 0 && (
              <div className="ev"><span className="muted">No on-chain activity yet for {account}. Claim loot or convert NXC to see it here.</span></div>
            )}
            {feed !== null && !feedDown && feed.map((e) => (
              <div className="ev" key={e.id}>
                <span className="ico" style={{ color: e.color }}>{e.icon}</span>
                <span><b>{e.verb}</b> {e.rest}</span>
                <span className="tx">{e.when}</span>
              </div>
            ))}
          </div>
          <div className="notice">Every entry is a real testnet transaction, indexed from on-chain NEP-297 events for <b>{account}</b>.</div>
        </div>

        <footer>
          <span className="tag">Live · testnet</span>
          <span>Reading real state from {CONTRACTS.assets} · {CONTRACTS.convert}</span>
        </footer>
      </div>
    </>
  );
}
