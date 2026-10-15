#!/usr/bin/env python3
import json
import subprocess
import sys

def main():
    print("Running MEV-NG Verification... This may take a moment.")
    
    # Run the verification via curl
    cmd = [
        "curl", "-s", "-H", "Content-Type: application/json",
        "-d", '{"cmd_line": "mev-ng verify --verify-flux --flash-loan --sample 10000", "params_json": "{}"}',
        "http://127.0.0.1:9877/mev/command"
    ]
    
    try:
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            print("Error executing verification command:", result.stderr)
            return
            
        data = json.loads(result.stdout)
    except Exception as e:
        print("Failed to run verification or parse JSON:", e)
        return

    results = data.get("results", [])
    if not results:
        print("No pools were verified.")
        return

    total_pools = len(results)
    matched_pools = sum(1 for r in results if r.get("all_matched", False))
    failed_pools = total_pools - matched_pools

    print(f"\n{'='*100}")
    print(f" MEV-NG FLUX VERIFICATION REPORT ")
    print(f"{'='*100}")
    
    for r in results:
        pool_pk = r.get("pool", "Unknown")
        dex = r.get("debug_info", {}).get("curve_info", {}).get("curve_type", "Unknown")
        all_matched = r.get("all_matched", False)
        
        status = "✅ PASS" if all_matched else "❌ FAIL"
        print(f"\n{status} | Pool: {pool_pk} | Curve: {dex}")
        print(f"{'-'*100}")
        print(f"{'Direction':<10} | {'Flux (Amount In)':<18} | {'Expected Out':<18} | {'Actual Out':<18} | {'Diff (%)':<10} | {'Match':<6} | {'Error'}")
        print(f"{'-'*100}")
        
        for t in r.get("tiers", []):
            direction = "A->B" if t.get("is_a_to_b", True) else "B->A"
            amt_in = str(t.get("amount_in", 0))
            exp_out = str(t.get("expected_out", 0))
            act_out = str(t.get("actual_out", 0))
            diff_pct = t.get("diff_pct", "0%")
            is_match = "Yes" if t.get("is_match", False) else "No"
            err = str(t.get("error")) if t.get("error") is not None else "None"
            
            print(f"{direction:<10} | {amt_in:<18} | {exp_out:<18} | {act_out:<18} | {diff_pct:<10} | {is_match:<6} | {err}")

    print(f"\n{'='*100}")
    print(f" VERIFICATION SUMMARY ")
    print(f"{'='*100}")
    print(f"Total Pools Tested : {total_pools}")
    print(f"Successfully Matched: {matched_pools}")
    print(f"Failed to Match    : {failed_pools}")
    print(f"Success Rate       : {(matched_pools/total_pools)*100:.2f}%")
    print(f"{'='*100}\n")

if __name__ == "__main__":
    main()
