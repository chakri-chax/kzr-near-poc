"use client";

import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { setupWalletSelector, type WalletSelector } from "@near-wallet-selector/core";
import { setupModal, type WalletSelectorModal } from "@near-wallet-selector/modal-ui";
import { setupMyNearWallet } from "@near-wallet-selector/my-near-wallet";
import { setupMeteorWallet } from "@near-wallet-selector/meteor-wallet";
import { CONTRACTS } from "./config";

interface Ctx {
  accountId: string | null;
  ready: boolean;
  signIn: () => void;
  signOut: () => Promise<void>;
  signAndSend: (
    receiverId: string,
    methodName: string,
    args: object,
    gasTgas: number,
    depositYocto: string,
  ) => Promise<unknown>;
}

const WalletCtx = createContext<Ctx | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [selector, setSelector] = useState<WalletSelector | null>(null);
  const [modal, setModal] = useState<WalletSelectorModal | null>(null);
  const [accountId, setAccountId] = useState<string | null>(null);

  useEffect(() => {
    let sub: { unsubscribe: () => void } | undefined;
    (async () => {
      const s = await setupWalletSelector({
        network: "testnet",
        modules: [setupMyNearWallet(), setupMeteorWallet()],
      });
      const m = setupModal(s, { contractId: CONTRACTS.coin });
      const apply = (state: { accounts: Array<{ accountId: string; active: boolean }> }) => {
        const acc = state.accounts.find((a) => a.active) ?? state.accounts[0];
        setAccountId(acc?.accountId ?? null);
      };
      apply(s.store.getState());
      sub = s.store.observable.subscribe(apply);
      setSelector(s);
      setModal(m);
    })();
    return () => sub?.unsubscribe();
  }, []);

  const value = useMemo<Ctx>(
    () => ({
      accountId,
      ready: !!selector,
      signIn: () => modal?.show(),
      signOut: async () => {
        const w = await selector?.wallet();
        await w?.signOut();
      },
      signAndSend: async (receiverId, methodName, args, gasTgas, depositYocto) => {
        const w = await selector!.wallet();
        return w.signAndSendTransaction({
          receiverId,
          actions: [
            {
              type: "FunctionCall",
              params: {
                methodName,
                args,
                gas: (BigInt(gasTgas) * 10n ** 12n).toString(),
                deposit: depositYocto,
              },
            },
          ],
        });
      },
    }),
    [accountId, selector, modal],
  );

  return <WalletCtx.Provider value={value}>{children}</WalletCtx.Provider>;
}

export function useWallet(): Ctx {
  const c = useContext(WalletCtx);
  if (!c) throw new Error("useWallet must be used within WalletProvider");
  return c;
}
