#!/usr/bin/env python3
import json
import subprocess
import time
import os

dex_list = [
    ("RaydiumV4", "raydium_v4", 50000),
    ("RaydiumCPMM", "raydium_cpmm", 50000),
    ("PumpSwap", "pump_swap", 50000),
    ("MeteoraDLMM", "meteora_dlmm", 50000),
    ("RaydiumCLMM", "raydium_clmm", 50000),
    ("OrcaWhirlpool", "orca_whirlpool", 50000),
    ("MeteoraDAMM", "meteora_damm", 50000),
    ("FluxBeam", "fluxbeam", 50000),
    ("PancakeSwap", "pancake_swap", 50000),
    ("Phoenix", "phoenix", 50000),
    ("SerumV3", "serum_v3", 50000),
]

def main():
    print("=" * 80)
    print("🚀 Starting 11-DEX Flux Verification Matrix")
    print("=" * 80)
    
    summary = []
    all_results = {}
    
    for dex_name, dex_arg, sample_count in dex_list:
        print(f"\n▶ Verifying {dex_name} (sample target: {sample_count})...")
        cmd = {
            "cmd_line": f"mev-ng verify_flux --dex {dex_arg} --limit {sample_count} --out /home/chy/Git/mainnet/data/flux_report_{dex_arg}.json",
            "params_json": "{}"
        }
        t0 = time.time()
        res = subprocess.run([
            "curl", "-s", "-H", "Content-Type: application/json",
            "-d", json.dumps(cmd), "http://127.0.0.1:9877/mev/command"
        ], capture_output=True, text=True)
        t_cost = time.time() - t0
        
        report_path = f"/home/chy/Git/mainnet/data/flux_report_{dex_arg}.json"
        if not os.path.exists(report_path):
            print(f"  ❌ Error: Report not found for {dex_name}")
            summary.append({
                "dex": dex_name,
                "error": "Report not found",
                "curl_output": res.stdout[:200]
            })
            continue
            
        with open(report_path) as f:
            rep = json.load(f)
            
        results = rep.get("results", [])
        tested = len(results)
        matched = sum(1 for r in results if r.get("pool_summary", {}).get("all_matched", False))
        total_tiers = sum(r.get("pool_summary", {}).get("total_tiers", 0) for r in results)
        matched_tiers = sum(r.get("pool_summary", {}).get("matched_tiers", 0) for r in results)
        
        all_results[dex_name] = results
        
        p_rate = (matched / tested * 100) if tested else 0.0
        t_rate = (matched_tiers / total_tiers * 100) if total_tiers else 0.0
        
        print(f"  ✅ {dex_name}: Pools {matched}/{tested} ({p_rate:.2f}%) | Tiers {matched_tiers}/{total_tiers} ({t_rate:.2f}%) | Time: {t_cost:.2f}s")
        
        summary.append({
            "dex": dex_name,
            "pools_tested": tested,
            "pools_matched": matched,
            "pool_rate": f"{p_rate:.2f}%",
            "tiers_tested": total_tiers,
            "tiers_matched": matched_tiers,
            "tier_rate": f"{t_rate:.2f}%",
            "time_sec": f"{t_cost:.2f}s"
        })

    print("\n" + "=" * 80)
    print("📊 11-DEX VERIFICATION MATRIX SUMMARY")
    print("=" * 80)
    print(f"{'DEX Protocol':<18} | {'Pools Matched':<15} | {'Pool Rate':<10} | {'Tiers Matched':<15} | {'Tier Rate':<10} | {'Time'}")
    print("-" * 80)
    
    total_pools_tested = sum(s.get("pools_tested", 0) for s in summary if "pools_tested" in s)
    total_pools_matched = sum(s.get("pools_matched", 0) for s in summary if "pools_matched" in s)
    total_tiers_tested = sum(s.get("tiers_tested", 0) for s in summary if "tiers_tested" in s)
    total_tiers_matched = sum(s.get("tiers_matched", 0) for s in summary if "tiers_matched" in s)
    
    for s in summary:
        if "error" in s:
            print(f"{s['dex']:<18} | ERROR: {s['error']}")
        else:
            p_str = f"{s['pools_matched']}/{s['pools_tested']}"
            t_str = f"{s['tiers_matched']}/{s['tiers_tested']}"
            print(f"{s['dex']:<18} | {p_str:<15} | {s['pool_rate']:<10} | {t_str:<15} | {s['tier_rate']:<10} | {s['time_sec']}")
            
    print("-" * 80)
    overall_p_rate = (total_pools_matched / total_pools_tested * 100) if total_pools_tested else 0.0
    overall_t_rate = (total_tiers_matched / total_tiers_tested * 100) if total_tiers_tested else 0.0
    print(f"{'TOTAL / OVERALL':<18} | {f'{total_pools_matched}/{total_pools_tested}':<15} | {f'{overall_p_rate:.2f}%':<10} | {f'{total_tiers_matched}/{total_tiers_tested}':<15} | {f'{overall_t_rate:.2f}%':<10} |")
    print("=" * 80)

    # Save summary json
    summary_path = "/home/chy/Git/mainnet/data/flux_verification_all_dexes.json"
    with open(summary_path, "w") as f:
        json.dump(summary, f, indent=2)
    print(f"\nSummary saved to {summary_path}")

if __name__ == "__main__":
    main()
