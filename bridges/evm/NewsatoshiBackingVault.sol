// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title NewsatoshiBackingVault — Immutable, Ownerless Multi-Chain Proof-of-Backing & Web4 Charter Registry
/// @notice Provides verifiable, un-ruggable on-chain reserve backing and immutable Web4 business charter
///         anchoring for Newsatoshi (`LOVE` / `NEWSATOSHI/2.1`) across Base, Arbitrum, Optimism & Polygon.
///         - ZERO owner, ZERO admin withdrawal, ZERO upgradeability, ZERO custody risk.
///         - Any Web4 autonomous business or sponsor can deposit native ETH/POL or ERC-20 (USDC, cbBTC, etc.)
///           tagged to a deterministic `charterId` (or `bytes32(0)` for the global Commons Backing Reserve).
///         - Any Web4 business can immutably anchor its `(constitutionHash, covenantRuleHash, backingBps, metadataUri)`
///           on-chain (first-writer-wins) so external agents and ERC-8004 indexers can verify reserves & covenants.
interface IERC20BackingMinimal {
    function balanceOf(address account) external view returns (uint256);
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

contract NewsatoshiBackingVault {
    /// Global Newsatoshi (`LOVE`) Commons Backing Reserve sentinel charter ID.
    bytes32 public constant GLOBAL_COMMONS_CHARTER = bytes32(0);
    /// Basis-points denominator (100.00% = 10_000).
    uint16 public constant MAX_BPS = 10_000;
    /// Maximum metadata URI byte length (prevents storage spam).
    uint256 public constant MAX_URI_BYTES = 256;

    struct CharterAnchor {
        address creator;
        bytes32 constitutionHash;
        bytes32 covenantRuleHash;
        uint16 backingBps;
        uint64 anchoredAt;
        string metadataUri;
    }

    /// Global cumulative verified backing reserve per token (`address(0)` = native ETH/POL).
    mapping(address => uint256) public totalReserveByToken;
    /// Per-charter cumulative verified backing reserve: `charterReserveByToken[charterId][token]`.
    mapping(bytes32 => mapping(address => uint256)) public charterReserveByToken;
    /// Total deposit events per charter.
    mapping(bytes32 => uint256) public charterDepositCount;
    /// Total global deposit events across all charters & tokens.
    uint256 public totalDeposits;
    /// Total anchored Web4 business charters on this chain.
    uint256 public totalAnchoredCharters;
    /// Immutable first-writer-wins charter anchors.
    mapping(bytes32 => CharterAnchor) public charterAnchors;

    uint256 private _locked = 1;

    event BackingDeposited(
        bytes32 indexed charterId,
        address indexed depositor,
        address indexed token,
        uint256 amount,
        bytes32 memoHash,
        uint256 newCharterTotal,
        uint256 newGlobalTotal
    );

    event CharterAnchored(
        bytes32 indexed charterId,
        address indexed creator,
        bytes32 constitutionHash,
        bytes32 covenantRuleHash,
        uint16 backingBps,
        string metadataUri
    );

    modifier nonReentrant() {
        require(_locked == 1, "REENTRANCY");
        _locked = 2;
        _;
        _locked = 1;
    }

    /// @notice Direct native ETH/POL transfers automatically credit the Global Commons Backing Reserve (`bytes32(0)`).
    receive() external payable nonReentrant {
        require(msg.value > 0, "AMOUNT_ZERO");
        _recordDeposit(GLOBAL_COMMONS_CHARTER, msg.sender, address(0), msg.value, bytes32(0));
    }

    /// @notice Permanently deposit native ETH/POL backing tagged to `charterId` (or `bytes32(0)` for Global Reserve).
    function depositEth(bytes32 charterId, bytes32 memoHash) external payable nonReentrant {
        require(msg.value > 0, "AMOUNT_ZERO");
        _recordDeposit(charterId, msg.sender, address(0), msg.value, memoHash);
    }

    /// @notice Permanently deposit ERC-20 backing (e.g. Base/Arb/OP/Polygon USDC) tagged to `charterId`.
    ///         Uses balance-before/after accounting so fee-on-transfer tokens are measured honestly.
    function depositErc20(
        bytes32 charterId,
        address token,
        uint256 amount,
        bytes32 memoHash
    ) external nonReentrant returns (uint256 received) {
        require(token != address(0), "INVALID_TOKEN");
        require(amount > 0, "AMOUNT_ZERO");

        uint256 beforeBal = IERC20BackingMinimal(token).balanceOf(address(this));
        _safeTransferFrom(token, msg.sender, address(this), amount);
        uint256 afterBal = IERC20BackingMinimal(token).balanceOf(address(this));
        require(afterBal > beforeBal, "ZERO_RECEIVED");
        received = afterBal - beforeBal;

        _recordDeposit(charterId, msg.sender, token, received, memoHash);
    }

    /// @notice Immutably anchor a Web4 Autonomous Business Charter on-chain (first-writer-wins; cannot be overwritten).
    function registerCharterAnchor(
        bytes32 charterId,
        bytes32 constitutionHash,
        bytes32 covenantRuleHash,
        uint16 backingBps,
        string calldata metadataUri
    ) external {
        require(charterId != GLOBAL_COMMONS_CHARTER, "RESERVED_CHARTER_ID");
        require(charterAnchors[charterId].creator == address(0), "CHARTER_ALREADY_ANCHORED");
        require(constitutionHash != bytes32(0), "ZERO_CONSTITUTION_HASH");
        require(covenantRuleHash != bytes32(0), "ZERO_COVENANT_HASH");
        require(backingBps <= MAX_BPS, "BPS_OUT_OF_RANGE");
        require(bytes(metadataUri).length > 0 && bytes(metadataUri).length <= MAX_URI_BYTES, "INVALID_URI_LEN");

        charterAnchors[charterId] = CharterAnchor({
            creator: msg.sender,
            constitutionHash: constitutionHash,
            covenantRuleHash: covenantRuleHash,
            backingBps: backingBps,
            anchoredAt: uint64(block.timestamp),
            metadataUri: metadataUri
        });
        totalAnchoredCharters += 1;

        emit CharterAnchored(
            charterId,
            msg.sender,
            constitutionHash,
            covenantRuleHash,
            backingBps,
            metadataUri
        );
    }

    /// @notice Single-call view returning global reserve total for `token` and global deposit & charter counts.
    function getReserveSummary(
        address token
    ) external view returns (uint256 totalReserve, uint256 depositsCount, uint256 anchoredChartersCount) {
        return (totalReserveByToken[token], totalDeposits, totalAnchoredCharters);
    }

    /// @notice Single-call view returning a charter's verifiable reserve, deposit count, and anchored covenant hashes.
    function getCharterSummary(
        bytes32 charterId,
        address token
    )
        external
        view
        returns (
            uint256 charterReserve,
            uint256 deposits,
            bytes32 constitutionHash,
            bytes32 covenantRuleHash,
            uint16 backingBps,
            address creator
        )
    {
        CharterAnchor storage a = charterAnchors[charterId];
        return (
            charterReserveByToken[charterId][token],
            charterDepositCount[charterId],
            a.constitutionHash,
            a.covenantRuleHash,
            a.backingBps,
            a.creator
        );
    }

    function _recordDeposit(
        bytes32 charterId,
        address depositor,
        address token,
        uint256 amount,
        bytes32 memoHash
    ) internal {
        uint256 newCharterTotal = charterReserveByToken[charterId][token] + amount;
        uint256 newGlobalTotal = totalReserveByToken[token] + amount;
        charterReserveByToken[charterId][token] = newCharterTotal;
        totalReserveByToken[token] = newGlobalTotal;
        charterDepositCount[charterId] += 1;
        totalDeposits += 1;

        emit BackingDeposited(
            charterId,
            depositor,
            token,
            amount,
            memoHash,
            newCharterTotal,
            newGlobalTotal
        );
    }

    function _safeTransferFrom(address token, address from, address to, uint256 value) internal {
        (bool ok, bytes memory ret) = token.call(
            abi.encodeWithSelector(IERC20BackingMinimal.transferFrom.selector, from, to, value)
        );
        require(ok && (ret.length == 0 || abi.decode(ret, (bool))), "ERC20_TRANSFER_FROM_FAILED");
    }
}
