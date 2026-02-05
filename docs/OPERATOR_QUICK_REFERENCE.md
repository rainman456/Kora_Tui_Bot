# Kora Nexus TUI - Operator Quick Reference

## Keyboard Commands

### Core Operations
| Key | Command | Description | Safety |
|-----|---------|-------------|--------|
| `s` | Scan | Discover sponsored accounts | Safe |
| `d` | Dry-Run | Simulate reclaim (no tx sent) | Safe |
| `e` | Execute | Reclaim selected account | ⚠️ Sends transaction |
| `x` | Export | Save to timestamped CSV | Safe |
| `w` | Whitelist | Protect account from reclaim | Safe |

### Navigation
| Key | Command | Description |
|-----|---------|-------------|
| `↑` / `k` | Up | Move selection up |
| `↓` / `j` | Down | Move selection down |
| `PgUp` | Page Up | Jump 10 rows up |
| `PgDn` | Page Down | Jump 10 rows down |

### Utility
| Key | Command | Description |
|-----|---------|-------------|
| `r` | Refresh | Update stats from database |
| `m` | Toggle Mode | Cycle Monitor/DryRun/Execute |
| `q` / `Esc` | Quit | Exit TUI |
| `Ctrl+C` | Force Quit | Emergency exit |

## Status Indicators

### Account Status
| Status | Meaning | Can Reclaim? |
|--------|---------|--------------|
| `ACTIVE` | Account is live | No |
| `ELIGIBLE` ⭐ | Ready to reclaim | Yes (after dry-run) |
| `WHITELISTED 🔒` | Protected | No |
| `RECLAIMED ✓` | Already reclaimed | No |
| `PROCESSING...` | Transaction pending | Wait |
| `FAILED ✗` | Operation failed | Retry |

### RPC Health
| Indicator | Meaning | Action |
|-----------|---------|--------|
| `✓ HEALTHY` | RPC working well | None |
| `⚠ DEGRADED` | Slow response | Monitor |
| `✗ DOWN` | RPC unreachable | Check config |

## Typical Workflow

### 1. Initial Scan
```
1. Press 's' to start scan
2. Wait for accounts to populate (real-time)
3. Review summary bar metrics
```

### 2. Review Account
```
1. Navigate with ↑↓ to select account
2. Check "Decision Panel" on right:
   - Eligibility reason
   - Account age
   - Rent amount
```

### 3. Dry-Run (Required)
```
1. Select ELIGIBLE account
2. Press 'd' for dry-run
3. Review results in Decision Panel:
   - Projected SOL
   - Estimated fee
   - Net gain
```

### 4. Execute Reclaim
```
1. Ensure dry-run completed (see panel)
2. Press 'e' to execute
3. Watch Activity Log for signature
4. Status changes to RECLAIMED ✓
```

### 5. Export Report
```
1. Press 'x' after scan complete
2. Find CSV: reclaim_report_YYYYMMDD_HHMMSS.csv
3. Share with team/accounting
```

## Safety Rules

### ⚠️ Before Execute
- [ ] Dry-run completed on THIS account
- [ ] Decision panel shows net gain > 0
- [ ] Account NOT whitelisted
- [ ] Status is ELIGIBLE

### 🔒 Whitelist Protection
- Whitelisted accounts CANNOT be reclaimed
- Use for critical integrations
- To whitelist: Select account → Press 'w'

### 💾 Data Persistence
- Whitelist saved to `whitelist.json`
- Reclaims saved to database
- Logs saved to `nexus.log`

## Layout Guide

```
┌─────────────────────────────────────────────────────────┐
│ Header                                                  │
│ Network | Mode | RPC Health                             │
├─────────────────────────────────────────────────────────┤
│ Summary Bar                                             │
│ [TOTAL] [LOCKED] [ELIGIBLE] [RECLAIMED] [AT-RISK]      │
├───────────────────────────────┬─────────────────────────┤
│ Monitor Table                 │ Decision Panel          │
│                               │                         │
│ List of accounts with:        │ Shows for selected:     │
│ - Address (truncated)         │ - Full address          │
│ - Program type                │ - Eligibility reason    │
│ - Account age                 │ - Dry-run results       │
│ - Rent locked (SOL)           │ - Safety warnings       │
│ - Last transaction            │                         │
│ - Current status              │                         │
│                               │                         │
│ ▶ = Current selection         │                         │
│                               │                         │
├───────────────────────────────┴─────────────────────────┤
│ Live Audit (Activity Log)                               │
│ - Recent operations with timestamps                     │
│ - Color-coded: Info/Success/Warning/Error               │
├─────────────────────────────────────────────────────────┤
│ Footer                                                  │
│ Keyboard legend | Treasury address                      │
└─────────────────────────────────────────────────────────┘
```

## Troubleshooting

### "Dry-run required before execute"
**Cause**: Trying to execute without dry-run  
**Fix**: Press 'd' on selected account first

### "Account is whitelisted"
**Cause**: Account in whitelist.json  
**Fix**: Edit whitelist.json or skip account

### RPC shows DOWN
**Cause**: Network issue or bad RPC URL  
**Fix**: Check config.toml, verify internet

### Scan finds no accounts
**Cause**: Wrong operator pubkey or no sponsored accounts  
**Fix**: Verify config.toml operator_pubkey

### Export fails
**Cause**: No write permissions  
**Fix**: Check current directory permissions

## Best Practices

### Daily Operations
1. Run scan in morning: `s`
2. Review eligible count in summary
3. Export report: `x`
4. Reclaim high-value accounts (>0.01 SOL)

### Safety First
- Always dry-run before execute
- Whitelist production accounts
- Review Activity Log regularly
- Export CSVs for audit trail

### Performance
- Scans are non-blocking (30 FPS)
- Rate-limited (max 10 concurrent RPC)
- Health checks every 10 seconds
- Logs to file (not stdout)

## File Locations

| File | Purpose | Format |
|------|---------|--------|
| `config.toml` | Configuration | TOML |
| `whitelist.json` | Protected accounts | JSON array |
| `nexus.log` | Application logs | Text |
| `reclaim_report_*.csv` | Exported reports | CSV |
| `kora.db` | Operation history | SQLite |

## Metrics Explained

### Summary Bar

**TOTAL**: All discovered accounts  
**LOCKED**: Total rent in all accounts (SOL)  
**ELIGIBLE**: Accounts ready to reclaim  
**RECLAIMED**: Total SOL recovered  
**AT-RISK**: SOL in accounts >30 days old  

### Monitor Table

**Age**: Days since account creation  
- `#d` = days
- `#m` = months (approx)
- `#y` = years (approx)

**Rent**: SOL locked as rent (6-9 decimals)

## Emergency Procedures

### TUI Frozen (Rare)
1. Press `Ctrl+C` to force quit
2. Check `nexus.log` for errors
3. Restart TUI

### RPC Down
1. Verify internet connection
2. Check RPC provider status
3. Update `config.toml` with backup RPC
4. Restart TUI

### Accidental Reclaim
- Reclaims are final (on-chain)
- Check Activity Log for signature
- Verify treasury received SOL

## Support

**Logs**: Check `nexus.log` for detailed traces  
**Config**: Verify `config.toml` settings  
**Database**: `kora.db` for operation history  
**Whitelist**: `whitelist.json` for protected accounts  

---

**Quick Start**: `s` → select → `d` → `e` → `x`  
**Safe Mode**: Always dry-run before execute  
**Export Often**: Regular CSV reports for compliance  

**Version**: 1.0.0  
**Last Updated**: 2026-02-02
