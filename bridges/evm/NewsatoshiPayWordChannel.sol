// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
pragma solidity ^0.8.24;

/// @title NewsatoshiPayWordChannel — Ownerless O(1) SHA-256 PayWords Stablecoin & ETH Settlement Channel
/// @notice Implements the capital-backed `batch-settlement` x402 v2 PayWords rail on EVM chains
///         (Base, Arbitrum, Optimism, Polygon, Ethereum) using the native `sha256` precompile (0x02):
///         - Payer locks `maxSteps * unitAmount` USDC (or ETH) against hash-chain root `w_0 = SHA256^N(w_N)`.
///         - Each HTTP 402 API call transmits only a 32-byte preimage `w_k` (zero per-call gas, zero signatures).
///         - Server settles thousands of calls in 1 on-chain `claim(channelId, steps, preimage)` tx at 0% protocol fee.
///         - Supports incremental chunked claims (up to 4,096 steps per tx), cooperative early `closeAndRefund`,
///           and unilateral payer `refund` strictly after `expiry`.
///         - Zero owner, zero admin keys, zero upgradeability, zero custody, fee-on-transfer protected.
interface IERC20PayWord {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 value) external returns (bool);
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

contract NewsatoshiPayWordChannel {
    uint64 public constant MAX_DURATION = 90 days;
    uint32 public constant MAX_STEPS_PER_CLAIM = 4096;

    struct Channel {
        address payer;
        address recipient;
        address token; // address(0) for native ETH, otherwise ERC-20 (e.g. Base USDC)
        bytes32 initialRoot;
        bytes32 currentRoot;
        uint256 unitAmount;
        uint32 maxSteps;
        uint32 claimedSteps;
        uint64 expiry;
        bool closed;
    }

    mapping(bytes32 => Channel) public channels;
    uint256 private _locked = 1;

    event ChannelOpened(
        bytes32 indexed channelId,
        address indexed payer,
        address indexed recipient,
        address token,
        bytes32 root,
        uint256 unitAmount,
        uint32 maxSteps,
        uint64 expiry
    );

    event ChannelClaimed(
        bytes32 indexed channelId,
        address indexed recipient,
        uint32 stepsClaimed,
        uint32 totalClaimedSteps,
        uint256 payoutAmount,
        bytes32 newRoot
    );

    event ChannelClosed(
        bytes32 indexed channelId,
        address indexed payer,
        uint256 refundedAmount,
        bool earlyClose
    );

    modifier nonReentrant() {
        require(_locked == 1, "REENTRANCY");
        _locked = 2;
        _;
        _locked = 1;
    }

    /// @notice Deterministic channel ID binding chain, contract, parties, token, root, pricing, and expiry.
    function computeChannelId(
        address payer,
        address recipient,
        address token,
        bytes32 root,
        uint256 unitAmount,
        uint32 maxSteps,
        uint64 expiry
    ) public view returns (bytes32) {
        return keccak256(
            abi.encode(block.chainid, address(this), payer, recipient, token, root, unitAmount, maxSteps, expiry)
        );
    }

    /// @notice Open a capital-backed SHA-256 PayWord channel denominated in ETH (`token == address(0)`)
    ///         or any standard ERC-20 stablecoin (e.g. Base USDC `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913`).
    function openChannel(
        address recipient,
        address token,
        bytes32 root,
        uint256 unitAmount,
        uint32 maxSteps,
        uint64 expiry
    ) external payable nonReentrant returns (bytes32 channelId) {
        require(recipient != address(0) && recipient != msg.sender, "INVALID_RECIPIENT");
        require(root != bytes32(0), "INVALID_ROOT");
        require(unitAmount > 0 && maxSteps > 0, "INVALID_TERMS");
        require(expiry > block.timestamp + 300, "EXPIRY_TOO_SOON");
        require(expiry <= block.timestamp + MAX_DURATION, "EXPIRY_TOO_FAR");

        uint256 totalDeposit = unitAmount * uint256(maxSteps);
        channelId = computeChannelId(msg.sender, recipient, token, root, unitAmount, maxSteps, expiry);
        require(channels[channelId].payer == address(0), "CHANNEL_EXISTS");

        if (token == address(0)) {
            require(msg.value == totalDeposit, "BAD_ETH_DEPOSIT");
        } else {
            require(msg.value == 0, "UNEXPECTED_ETH");
            uint256 balBefore = IERC20PayWord(token).balanceOf(address(this));
            _safeTransferFrom(token, msg.sender, address(this), totalDeposit);
            uint256 balAfter = IERC20PayWord(token).balanceOf(address(this));
            require(balAfter >= balBefore && balAfter - balBefore == totalDeposit, "FEE_ON_TRANSFER_UNSUPPORTED");
        }

        channels[channelId] = Channel({
            payer: msg.sender,
            recipient: recipient,
            token: token,
            initialRoot: root,
            currentRoot: root,
            unitAmount: unitAmount,
            maxSteps: maxSteps,
            claimedSteps: 0,
            expiry: expiry,
            closed: false
        });

        emit ChannelOpened(channelId, msg.sender, recipient, token, root, unitAmount, maxSteps, expiry);
    }

    /// @notice Pure SHA-256 hash-chain verification helper (`SHA256^steps(preimage) == expectedRoot`).
    function verifyHashChain(bytes32 preimage, uint32 steps, bytes32 expectedRoot) public pure returns (bool) {
        if (steps == 0 || steps > MAX_STEPS_PER_CLAIM) return false;
        bytes32 h = preimage;
        for (uint32 i = 0; i < steps; ) {
            h = sha256(abi.encodePacked(h));
            unchecked {
                ++i;
            }
        }
        return h == expectedRoot;
    }

    /// @notice Settle `steps` accumulated API calls by revealing the latest SHA-256 preimage `w_k`.
    ///         Advances `currentRoot = preimage` so large sessions can be settled incrementally or in one tx.
    ///         Funds ALWAYS route strictly to `channel.recipient` with 0% protocol fee.
    function claim(bytes32 channelId, uint32 steps, bytes32 preimage) external nonReentrant {
        Channel storage c = channels[channelId];
        require(c.payer != address(0), "CHANNEL_NOT_FOUND");
        require(!c.closed, "CHANNEL_CLOSED");
        require(block.timestamp < c.expiry, "CHANNEL_EXPIRED");
        require(steps > 0 && steps <= MAX_STEPS_PER_CLAIM, "BAD_STEPS");
        uint32 newClaimed = c.claimedSteps + steps;
        require(newClaimed <= c.maxSteps, "OVERCLAIM");
        require(verifyHashChain(preimage, steps, c.currentRoot), "BAD_PAYWORD_PROOF");

        c.currentRoot = preimage;
        c.claimedSteps = newClaimed;
        if (newClaimed == c.maxSteps) {
            c.closed = true;
        }

        uint256 payout = uint256(steps) * c.unitAmount;
        _payout(c.token, c.recipient, payout);

        emit ChannelClaimed(channelId, c.recipient, steps, newClaimed, payout, preimage);
        if (c.closed) {
            emit ChannelClosed(channelId, c.payer, 0, false);
        }
    }

    /// @notice Cooperative early close triggered strictly by `recipient`, immediately returning
    ///         any unused deposit `(maxSteps - claimedSteps) * unitAmount` back to `payer`.
    function closeAndRefund(bytes32 channelId) external nonReentrant {
        Channel storage c = channels[channelId];
        require(c.payer != address(0), "CHANNEL_NOT_FOUND");
        require(!c.closed, "CHANNEL_CLOSED");
        require(msg.sender == c.recipient, "ONLY_RECIPIENT");

        c.closed = true;
        uint256 refundAmount = uint256(c.maxSteps - c.claimedSteps) * c.unitAmount;
        if (refundAmount > 0) {
            _payout(c.token, c.payer, refundAmount);
        }

        emit ChannelClosed(channelId, c.payer, refundAmount, true);
    }

    /// @notice Unilateral refund of all unclaimed funds back to `payer` strictly after `expiry`.
    function refund(bytes32 channelId) external nonReentrant {
        Channel storage c = channels[channelId];
        require(c.payer != address(0), "CHANNEL_NOT_FOUND");
        require(!c.closed, "CHANNEL_CLOSED");
        require(block.timestamp >= c.expiry, "CHANNEL_ACTIVE");

        c.closed = true;
        uint256 refundAmount = uint256(c.maxSteps - c.claimedSteps) * c.unitAmount;
        if (refundAmount > 0) {
            _payout(c.token, c.payer, refundAmount);
        }

        emit ChannelClosed(channelId, c.payer, refundAmount, false);
    }

    /// @notice Read remaining claimable steps and remaining locked balance for a channel.
    function remaining(bytes32 channelId) external view returns (uint32 remainingSteps, uint256 remainingAmount) {
        Channel storage c = channels[channelId];
        if (c.payer == address(0) || c.closed) return (0, 0);
        remainingSteps = c.maxSteps - c.claimedSteps;
        remainingAmount = uint256(remainingSteps) * c.unitAmount;
    }

    function _payout(address token, address to, uint256 amount) internal {
        if (token == address(0)) {
            (bool ok, ) = payable(to).call{value: amount}("");
            require(ok, "ETH_TRANSFER_FAILED");
        } else {
            _safeTransfer(token, to, amount);
        }
    }

    function _safeTransfer(address token, address to, uint256 amount) internal {
        (bool ok, bytes memory data) = token.call(
            abi.encodeWithSelector(IERC20PayWord.transfer.selector, to, amount)
        );
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "ERC20_TRANSFER_FAILED");
    }

    function _safeTransferFrom(address token, address from, address to, uint256 amount) internal {
        (bool ok, bytes memory data) = token.call(
            abi.encodeWithSelector(IERC20PayWord.transferFrom.selector, from, to, amount)
        );
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "ERC20_TRANSFER_FROM_FAILED");
    }
}
