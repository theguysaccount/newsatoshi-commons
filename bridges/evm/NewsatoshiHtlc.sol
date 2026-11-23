// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title NewsatoshiHtlc — Immutable, Ownerless SHA-256 HTLC Bridge for Base / Ethereum / Arbitrum / Optimism / Polygon
/// @notice Enables 100% trustless atomic swaps between native ETH or any ERC-20 (USDC, USDT, cbBTC, WBTC)
///         and Newsatoshi L1 (`LOVE`) via identical `sha256(32-byte preimage) == paymentHash` hash-locks.
///         - Zero admin keys, zero upgradeability, zero custody, zero wrapped tokens.
///         - Uses the EVM native `sha256` precompile (0x02), byte-compatible with `newsatoshi lock --hash`.
interface IERC20Minimal {
    function transfer(address to, uint256 value) external returns (bool);
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

contract NewsatoshiHtlc {
    struct SwapLock {
        address sender;
        address recipient;
        address token; // address(0) for native ETH, otherwise ERC-20 (e.g. Base USDC)
        uint256 amount;
        bytes32 paymentHash;
        uint64 timelock; // Unix timestamp (seconds) after which sender may refund
        bool claimed;
        bool refunded;
        bytes32 preimage;
    }

    mapping(bytes32 => SwapLock) public swaps;
    uint256 private _locked = 1;

    event Locked(
        bytes32 indexed swapId,
        bytes32 indexed paymentHash,
        address indexed sender,
        address recipient,
        address token,
        uint256 amount,
        uint64 timelock
    );

    event Claimed(
        bytes32 indexed swapId,
        bytes32 indexed paymentHash,
        address indexed recipient,
        bytes32 preimage
    );

    event Refunded(
        bytes32 indexed swapId,
        bytes32 indexed paymentHash,
        address indexed sender
    );

    modifier nonReentrant() {
        require(_locked == 1, "REENTRANCY");
        _locked = 2;
        _;
        _locked = 1;
    }

    /// @notice Deterministic swapId binding all immutable HTLC terms.
    function computeSwapId(
        address sender,
        address recipient,
        address token,
        uint256 amount,
        bytes32 paymentHash,
        uint64 timelock
    ) public view returns (bytes32) {
        return keccak256(abi.encode(block.chainid, address(this), sender, recipient, token, amount, paymentHash, timelock));
    }

    /// @notice Lock native ETH into an atomic HTLC claimable with `preimage` where `sha256(preimage) == paymentHash`.
    function lockEth(
        address recipient,
        bytes32 paymentHash,
        uint64 timelock
    ) external payable nonReentrant returns (bytes32 swapId) {
        require(msg.value > 0, "AMOUNT_ZERO");
        require(recipient != address(0) && recipient != msg.sender, "INVALID_RECIPIENT");
        require(paymentHash != bytes32(0), "INVALID_HASH");
        require(timelock > block.timestamp + 300, "TIMELOCK_TOO_SOON");

        swapId = computeSwapId(msg.sender, recipient, address(0), msg.value, paymentHash, timelock);
        require(swaps[swapId].sender == address(0), "SWAP_EXISTS");

        swaps[swapId] = SwapLock({
            sender: msg.sender,
            recipient: recipient,
            token: address(0),
            amount: msg.value,
            paymentHash: paymentHash,
            timelock: timelock,
            claimed: false,
            refunded: false,
            preimage: bytes32(0)
        });

        emit Locked(swapId, paymentHash, msg.sender, recipient, address(0), msg.value, timelock);
    }

    /// @notice Lock any ERC-20 token (e.g. Base/Arbitrum/Mainnet USDC or cbBTC) into an atomic HTLC.
    function lockErc20(
        address recipient,
        address token,
        uint256 amount,
        bytes32 paymentHash,
        uint64 timelock
    ) external nonReentrant returns (bytes32 swapId) {
        require(token != address(0), "INVALID_TOKEN");
        require(amount > 0, "AMOUNT_ZERO");
        require(recipient != address(0) && recipient != msg.sender, "INVALID_RECIPIENT");
        require(paymentHash != bytes32(0), "INVALID_HASH");
        require(timelock > block.timestamp + 300, "TIMELOCK_TOO_SOON");

        swapId = computeSwapId(msg.sender, recipient, token, amount, paymentHash, timelock);
        require(swaps[swapId].sender == address(0), "SWAP_EXISTS");

        _safeTransferFrom(token, msg.sender, address(this), amount);

        swaps[swapId] = SwapLock({
            sender: msg.sender,
            recipient: recipient,
            token: token,
            amount: amount,
            paymentHash: paymentHash,
            timelock: timelock,
            claimed: false,
            refunded: false,
            preimage: bytes32(0)
        });

        emit Locked(swapId, paymentHash, msg.sender, recipient, token, amount, timelock);
    }

    /// @notice Release locked funds to `recipient` by revealing the 32-byte SHA-256 preimage.
    ///         Anyone may relay the preimage; funds ALWAYS go strictly to `swap.recipient`.
    function claim(bytes32 swapId, bytes32 preimage) external nonReentrant {
        SwapLock storage s = swaps[swapId];
        require(s.sender != address(0), "SWAP_NOT_FOUND");
        require(!s.claimed && !s.refunded, "ALREADY_SETTLED");
        require(block.timestamp < s.timelock, "TIMELOCK_EXPIRED");
        require(sha256(abi.encodePacked(preimage)) == s.paymentHash, "BAD_PREIMAGE");

        s.claimed = true;
        s.preimage = preimage;

        if (s.token == address(0)) {
            (bool ok, ) = payable(s.recipient).call{value: s.amount}("");
            require(ok, "ETH_TRANSFER_FAILED");
        } else {
            _safeTransfer(s.token, s.recipient, s.amount);
        }

        emit Claimed(swapId, s.paymentHash, s.recipient, preimage);
    }

    /// @notice Refund locked funds back to `sender` strictly after `timelock` has passed.
    function refund(bytes32 swapId) external nonReentrant {
        SwapLock storage s = swaps[swapId];
        require(s.sender != address(0), "SWAP_NOT_FOUND");
        require(!s.claimed && !s.refunded, "ALREADY_SETTLED");
        require(block.timestamp >= s.timelock, "TIMELOCK_ACTIVE");

        s.refunded = true;

        if (s.token == address(0)) {
            (bool ok, ) = payable(s.sender).call{value: s.amount}("");
            require(ok, "ETH_REFUND_FAILED");
        } else {
            _safeTransfer(s.token, s.sender, s.amount);
        }

        emit Refunded(swapId, s.paymentHash, s.sender);
    }

    function _safeTransfer(address token, address to, uint256 amount) internal {
        (bool ok, bytes memory data) = token.call(
            abi.encodeWithSelector(IERC20Minimal.transfer.selector, to, amount)
        );
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "ERC20_TRANSFER_FAILED");
    }

    function _safeTransferFrom(address token, address from, address to, uint256 amount) internal {
        (bool ok, bytes memory data) = token.call(
            abi.encodeWithSelector(IERC20Minimal.transferFrom.selector, from, to, amount)
        );
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "ERC20_TRANSFER_FROM_FAILED");
    }
}
