const STORAGE_KEY = 'cerp_positions'

export interface StoredPosition {
  commitment: string  // 64-char hex
  symbol: string
  side: 0 | 1
  leverage: number
  openedAt: number    // Date.now()
}

function load(): StoredPosition[] {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '[]')
  } catch {
    return []
  }
}

function save(positions: StoredPosition[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(positions))
}

export const positionsStore = {
  all(): StoredPosition[] {
    return load()
  },

  add(p: StoredPosition) {
    const existing = load()
    if (!existing.find((x) => x.commitment === p.commitment)) {
      save([...existing, p])
    }
  },

  remove(commitment: string) {
    save(load().filter((p) => p.commitment !== commitment))
  },

  forWallet(_publicKey: string): StoredPosition[] {
    // All positions in this browser session belong to the connected wallet.
    // If multi-account support is needed later, key by publicKey.
    return load()
  },
}
