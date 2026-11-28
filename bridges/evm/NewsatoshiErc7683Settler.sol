// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title NewsatoshiErc7683Settler — Immutable, Ownerless ERC-7683 Origin Settler for Newsatoshi SHA-256 Atomic Swaps
/// @notice Bridges ERC-7683 solver networks (UniswapX, Across, CoW, Superchain) directly into
///         Newsatoshi Mainnet v2 (`LOVE` / `nsUSD` / custom RFC-0028 stablecoins) with:
///         - Zero owner, zero admin, zero upgradeability, zero protocol fee (0.00%).
///         - Canonical ERC-7683 `GaslessCrossChainOrder` + `OnchainCrossChainOrder` interfaces.
///         - Authentic EIP-712 `NewsatoshiERC7683OriginSettler` v1 domain matching `newsatoshi-commons/src/interop.rs`.
///         - Trustless settlement via EVM native `sha256` precompile (0x02): revealing the 32-byte preimage
///           on Newsatoshi Mainnet v2 unlocks the escrowed EVM ERC-20 directly to the solver.

interface IERC20Settler {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 value) external returns (bool);
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

struct GaslessCrossChainOrder {
    address originSettler;
    address user;
    uint256 nonce;
    uint256 originChainId;
    uint32 openDeadline;
    uint32 fillDeadline;
    bytes32 orderDataType;
    bytes orderData;
}

struct OnchainCrossChainOrder {
    uint32 fillDeadline;
    bytes32 orderDataType;
    bytes orderData;
}

struct NewsatoshiOrderData {
    address inputToken;
    uint256 inputAmount;
    bytes32 paymentHash;
    bytes32 destinationNs1Hash;
}

struct EscrowedOrder {
    address user;
    address solver;
    address inputToken;
    uint256 inputAmount;
    bytes32 paymentHash;
    bytes32 destinationNs1Hash;
    uint32 fillDeadline;
    bool claimed;
    bool refunded;
    bytes32 preimage;
}

contract NewsatoshiErc7683Settler {
    uint32 public constant MAX_FILL_WINDOW = 30 days;

    bytes32 public constant EIP712_DOMAIN_TYPEHASH =
        keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
    bytes32 public constant DOMAIN_NAME_HASH = keccak256("NewsatoshiERC7683OriginSettler");
    bytes32 public constant DOMAIN_VERSION_HASH = keccak256("1");

    bytes32 public constant GASLESS_ORDER_TYPEHASH = keccak256(
        "GaslessCrossChainOrder(address originSettler,address user,uint256 nonce,uint256 originChainId,uint32 openDeadline,uint32 fillDeadline,bytes32 orderDataType,bytes orderData)"
    );
    bytes32 public constant NEWSATOSHI_ORDER_DATA_TYPE = keccak256(
        "NewsatoshiSha256HtlcOrder(address inputToken,uint256 inputAmount,bytes32 paymentHash,bytes32 destinationNs1Hash)"
    );

    /// Half order of secp256k1 curve to prevent ECDSA signature malleability (EIP-2).
    uint256 private constant SECP256K1_HALF_N =
        0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0;

    mapping(bytes32 => EscrowedOrder) public orders;
    mapping(address => mapping(uint256 => bool)) public usedNonces;
    uint256 private _locked = 1;

    event Open(
        bytes32 indexed orderId,
        address indexed user,
        address indexed solver,
        address inputToken,
        uint256 inputAmount,
        bytes32 paymentHash,
        uint32 fillDeadline
    );

    event Claimed(
        bytes32 indexed orderId,
        bytes32 indexed paymentHash,
        address indexed solver,
        bytes32 preimage
    );

    event Refunded(
        bytes32 indexed orderId,
        bytes32 indexed paymentHash,
        address indexed user
    );

    modifier nonReentrant() {
        require(_locked == 1, "REENTRANCY");
        _locked = 2;
        _;
        _locked = 1;
    }

    function DOMAIN_SEPARATOR() public view returns (bytes32) {
        return keccak256(
            abi.encode(
                EIP712_DOMAIN_TYPEHASH,
                DOMAIN_NAME_HASH,
                DOMAIN_VERSION_HASH,
                block.chainid,
                address(this)
            )
        );
    }

    function hashGaslessOrder(GaslessCrossChainOrder calldata order) public view returns (bytes32) {
        bytes32 structHash = keccak256(
            abi.encode(
                GASLESS_ORDER_TYPEHASH,
                order.originSettler,
                order.user,
                order.nonce,
                order.originChainId,
                order.openDeadline,
                order.fillDeadline,
                order.orderDataType,
                keccak256(order.orderData)
            )
        );
        return keccak256(abi.encodePacked("\x19\x01", DOMAIN_SEPARATOR(), structHash));
    }

    /// @notice ERC-7683 gasless `openFor`: solver submits user's EIP-712 signature and locks user's ERC-20
    ///         into a zero-custody SHA-256 HTLC payable strictly to `solver` upon revealing `preimage`.
    function openFor(
        GaslessCrossChainOrder calldata order,
        bytes calldata signature,
        bytes calldata originFillerData
    ) external nonReentrant returns (bytes32 orderId) {
        require(order.originSettler == address(this), "WRONG_SETTLER");
        require(order.originChainId == block.chainid, "WRONG_CHAIN");
        require(order.user != address(0), "ZERO_USER");
        require(block.timestamp <= order.openDeadline, "OPEN_DEADLINE_EXPIRED");
        require(order.fillDeadline > block.timestamp + 300, "FILL_DEADLINE_TOO_SOON");
        require(order.fillDeadline <= block.timestamp + MAX_FILL_WINDOW, "FILL_DEADLINE_TOO_FAR");
        require(order.orderDataType == NEWSATOSHI_ORDER_DATA_TYPE, "UNSUPPORTED_ORDER_TYPE");
        require(!usedNonces[order.user][order.nonce], "NONCE_USED");

        bytes32 digest = hashGaslessOrder(order);
        address recovered = _recoverSigner(digest, signature);
        require(recovered == order.user, "INVALID_SIGNATURE");
        usedNonces[order.user][order.nonce] = true;

        address solver = originFillerData.length == 32 ? abi.decode(originFillerData, (address)) : msg.sender;
        require(solver != address(0) && solver != order.user, "INVALID_SOLVER");

        NewsatoshiOrderData memory d = abi.decode(order.orderData, (NewsatoshiOrderData));
        orderId = _escrowOrder(order.user, solver, d, order.fillDeadline, digest);
    }

    /// @notice ERC-7683 direct on-chain `open`: user opens an order directly, designating a solver or msg.sender's counterparty.
    function open(
        OnchainCrossChainOrder calldata order,
        address solver
    ) external nonReentrant returns (bytes32 orderId) {
        require(solver != address(0) && solver != msg.sender, "INVALID_SOLVER");
        require(order.fillDeadline > block.timestamp + 300, "FILL_DEADLINE_TOO_SOON");
        require(order.fillDeadline <= block.timestamp + MAX_FILL_WINDOW, "FILL_DEADLINE_TOO_FAR");
        require(order.orderDataType == NEWSATOSHI_ORDER_DATA_TYPE, "UNSUPPORTED_ORDER_TYPE");

        NewsatoshiOrderData memory d = abi.decode(order.orderData, (NewsatoshiOrderData));
        bytes32 salt = keccak256(abi.encode(block.chainid, address(this), msg.sender, solver, order.fillDeadline, order.orderData));
        orderId = _escrowOrder(msg.sender, solver, d, order.fillDeadline, salt);
    }

    /// @notice Unlocks escrowed ERC-20 tokens strictly to `order.solver` when `sha256(preimage) == paymentHash`.
    function claimWithPreimage(bytes32 orderId, bytes32 preimage) external nonReentrant {
        EscrowedOrder storage o = orders[orderId];
        require(o.user != address(0), "ORDER_NOT_FOUND");
        require(!o.claimed && !o.refunded, "ORDER_SETTLED");
        require(sha256(abi.encodePacked(preimage)) == o.paymentHash, "INVALID_PREIMAGE");

        o.claimed = true;
        o.preimage = preimage;
        _safeTransfer(o.inputToken, o.solver, o.inputAmount);

        emit Claimed(orderId, o.paymentHash, o.solver, preimage);
    }

    /// @notice Refunds 100% of escrowed ERC-20 tokens strictly back to `order.user` after `fillDeadline`.
    function refundExpired(bytes32 orderId) external nonReentrant {
        EscrowedOrder storage o = orders[orderId];
        require(o.user != address(0), "ORDER_NOT_FOUND");
        require(!o.claimed && !o.refunded, "ORDER_SETTLED");
        require(block.timestamp >= o.fillDeadline, "NOT_EXPIRED");

        o.refunded = true;
        _safeTransfer(o.inputToken, o.user, o.inputAmount);

        emit Refunded(orderId, o.paymentHash, o.user);
    }

    function _escrowOrder(
        address user,
        address solver,
        NewsatoshiOrderData memory d,
        uint32 fillDeadline,
        bytes32 salt
    ) internal returns (bytes32 orderId) {
        require(d.inputToken != address(0), "INVALID_TOKEN");
        require(d.inputAmount > 0, "AMOUNT_ZERO");
        require(d.paymentHash != bytes32(0), "INVALID_HASH");

        orderId = keccak256(abi.encode(block.chainid, address(this), salt, d.paymentHash));
        require(orders[orderId].user == address(0), "ORDER_EXISTS");

        uint256 balBefore = IERC20Settler(d.inputToken).balanceOf(address(this));
        _safeTransferFrom(d.inputToken, user, address(this), d.inputAmount);
        uint256 received = IERC20Settler(d.inputToken).balanceOf(address(this)) - balBefore;
        require(received > 0, "ZERO_RECEIVED");

        orders[orderId] = EscrowedOrder({
            user: user,
            solver: solver,
            inputToken: d.inputToken,
            inputAmount: received,
            paymentHash: d.paymentHash,
            destinationNs1Hash: d.destinationNs1Hash,
            fillDeadline: fillDeadline,
            claimed: false,
            refunded: false,
            preimage: bytes32(0)
        });

        emit Open(orderId, user, solver, d.inputToken, received, d.paymentHash, fillDeadline);
    }

    function _recoverSigner(bytes32 digest, bytes calldata sig) internal pure returns (address) {
        require(sig.length == 65, "INVALID_SIG_LENGTH");
        bytes32 r;
        bytes32 s;
        uint8 v;
        assembly {
            r := calldataload(sig.offset)
            s := calldataload(add(sig.offset, 32))
            v := byte(0, calldataload(add(sig.offset, 64)))
        }
        require(uint256(s) <= SECP256K1_HALF_N, "MALLEABLE_SIG");
        require(v == 27 || v == 28, "INVALID_V");
        address signer = ecrecover(digest, v, r, s);
        require(signer != address(0), "BAD_RECOVER");
        return signer;
    }

    function _safeTransfer(address token, address to, uint256 value) internal {
        (bool ok, bytes memory data) = token.call(abi.encodeWithSelector(IERC20Settler.transfer.selector, to, value));
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "TRANSFER_FAILED");
    }

    function _safeTransferFrom(address token, address from, address to, uint256 value) internal {
        (bool ok, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20Settler.transferFrom.selector, from, to, value));
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "TRANSFER_FROM_FAILED");
    }
}
