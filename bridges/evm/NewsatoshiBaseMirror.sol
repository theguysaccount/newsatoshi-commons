// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title NewsatoshiBaseMirror (wLOVE) — Immutable Base ERC-20 Mirror & HTLC Bridge Asset for Newsatoshi (LOVE)
/// @notice Zero-owner, zero-tax, non-mintable ERC-20 deployed for permissionless Base DEX liquidity discovery
///         and trustless SHA-256 atomic swapping into native Newsatoshi L1 (LOVE) via NewsatoshiHtlc.lockErc20().
/// @dev    100% of the fixed 21,000,000 wLOVE supply is seeded directly into a full-range Uniswap V3 WETH pool
///         on Base Mainnet whose LP NFT is permanently burned to 0x000000000000000000000000000000000000dEaD.
///         - Main site:         https://newsatoshi.org
///         - Commons gateway:   https://commons.newsatoshi.org
///         - MCP server (14):   https://commons.newsatoshi.org/mcp
///         - Whitepaper (2.1):  https://commons.newsatoshi.org/whitepaper
///         - Lean 4.36 proof:   https://commons.newsatoshi.org/formal/NewsatoshiCovenants.lean
///         - ERC-8004 identity: https://commons.newsatoshi.org/.well-known/erc8004.json (Base Agent #1229992)
contract NewsatoshiBaseMirror {
    string public constant name = "Newsatoshi (Base Mirror)";
    string public constant symbol = "wLOVE";
    uint8 public constant decimals = 18;
    uint256 public constant totalSupply = 21_000_000 * 1e18;

    // Immutable on-chain protocol discovery pointers
    string public constant SITE_URL = "https://newsatoshi.org";
    string public constant COMMONS_URL = "https://commons.newsatoshi.org";
    string public constant MCP_ENDPOINT = "https://commons.newsatoshi.org/mcp";
    string public constant WHITEPAPER_URL = "https://commons.newsatoshi.org/whitepaper";
    string public constant LEAN4_PROOF_URL = "https://commons.newsatoshi.org/formal/NewsatoshiCovenants.lean";
    string public constant ERC8004_URI = "https://commons.newsatoshi.org/.well-known/erc8004.json";

    address public constant CANONICAL_HTLC = 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987;
    address public constant CANONICAL_BACKING_VAULT = 0x37Ed94525DBE408fA72cDd156409875666679265;
    address public constant ERC8004_IDENTITY_REGISTRY = 0x8004A169FB4a3325136EB29fA0ceB6D2e539a432;
    uint256 public constant BASE_ERC8004_AGENT_ID = 1229992;

    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    error InsufficientBalance();
    error InsufficientAllowance();
    error ZeroAddress();

    constructor() {
        balanceOf[msg.sender] = totalSupply;
        emit Transfer(address(0), msg.sender, totalSupply);
    }

    function approve(address spender, uint256 value) external returns (bool) {
        allowance[msg.sender][spender] = value;
        emit Approval(msg.sender, spender, value);
        return true;
    }

    function transfer(address to, uint256 value) external returns (bool) {
        _transfer(msg.sender, to, value);
        return true;
    }

    function transferFrom(address from, address to, uint256 value) external returns (bool) {
        uint256 allowed = allowance[from][msg.sender];
        if (allowed != type(uint256).max) {
            if (allowed < value) revert InsufficientAllowance();
            unchecked {
                allowance[from][msg.sender] = allowed - value;
            }
        }
        _transfer(from, to, value);
        return true;
    }

    function _transfer(address from, address to, uint256 value) internal {
        if (to == address(0)) revert ZeroAddress();
        uint256 bal = balanceOf[from];
        if (bal < value) revert InsufficientBalance();
        unchecked {
            balanceOf[from] = bal - value;
            balanceOf[to] += value;
        }
        emit Transfer(from, to, value);
    }
}
