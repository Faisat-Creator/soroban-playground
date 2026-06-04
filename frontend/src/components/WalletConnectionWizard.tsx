"use client";

import React from "react";
import { Wallet, CheckCircle2, AlertCircle, ExternalLink, Loader2 } from "lucide-react";
import { useWallet, WalletType } from "./providers/WalletProvider";

interface WalletOption {
  id: WalletType;
  name: string;
  icon: React.ReactNode;
  installUrl: string;
}

export default function WalletConnectionWizard() {
  const { connect, activeWallet, status, error, isWalletDetected } = useWallet();

  const wallets: WalletOption[] = [
    {
      id: "freighter",
      name: "Freighter",
      icon: <Wallet className="text-cyan-400" size={20} />,
      installUrl: "https://freighter.app",
    },
    {
      id: "soroban-wallet",
      name: "Soroban Wallet",
      icon: <Wallet className="text-orange-400" size={20} />,
      installUrl: "https://sorobanwallet.com", // Placeholder
    },
  ];

  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2">
        {wallets.map((wallet) => {
          const isDetected = isWalletDetected(wallet.id);
          const isActive = activeWallet === wallet.id && status === "connected";
          const isConnecting = activeWallet === wallet.id && status === "connecting";

          return (
            <div
              key={wallet.id}
              className={`relative flex flex-col p-5 rounded-2xl border transition-all ${
                isActive
                  ? "bg-cyan-500/10 border-cyan-500/50 ring-1 ring-cyan-500/20"
                  : "bg-white/5 border-white/10 hover:border-white/20"
              }`}
            >
              <div className="flex items-start justify-between mb-4">
                <div className={`p-3 rounded-xl ${isActive ? "bg-cyan-500/20" : "bg-white/5"}`}>
                  {wallet.icon}
                </div>
                {isActive && <CheckCircle2 className="text-cyan-400" size={20} />}
                {isConnecting && <Loader2 className="text-cyan-400 animate-spin" size={20} />}
              </div>

              <h3 className="text-lg font-semibold text-white mb-1">{wallet.name}</h3>
              
              {!isDetected ? (
                <div className="mt-auto space-y-3">
                  <p className="text-xs text-slate-400">Not detected in your browser.</p>
                  <a
                    href={wallet.installUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="flex items-center gap-2 text-xs font-medium text-cyan-400 hover:text-cyan-300 transition-colors"
                  >
                    <ExternalLink size={14} />
                    Install Extension
                  </a>
                </div>
              ) : (
                <button
                  onClick={() => connect(wallet.id)}
                  disabled={status === "connecting"}
                  className={`mt-auto w-full py-2.5 rounded-xl text-sm font-semibold transition-all ${
                    isActive
                      ? "bg-cyan-500/20 text-cyan-200 cursor-default"
                      : "bg-white/10 text-white hover:bg-white/20 active:scale-95"
                  }`}
                >
                  {isActive ? "Connected" : isConnecting ? "Connecting..." : "Connect Wallet"}
                </button>
              )}
            </div>
          );
        })}
      </div>

      {error && (
        <div className="flex items-start gap-3 p-4 rounded-xl bg-rose-500/10 border border-rose-500/20 text-rose-300 text-sm">
          <AlertCircle className="shrink-0 mt-0.5" size={18} />
          <p>{error}</p>
        </div>
      )}
    </div>
  );
}
