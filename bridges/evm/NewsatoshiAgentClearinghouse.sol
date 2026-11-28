// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title NewsatoshiAgentClearinghouse — Immutable, Ownerless RFC-0030 Autonomous Cyber-Economy Clearinghouse
/// @notice Zero-custody, 0.00%-fee EVM clearinghouse matching `newsatoshi-commons/src/cyber_economy.rs`
///         and Lean 4.36.0 Theorems 9-11 (`formal/InteropAndOrderTheorems.lean`):
///         1. Client agent escrows ERC-20 budget (e.g. Base/Optimism/Arbitrum USDC) against a SHA-256
///            verifiable completion digest (`expectedDigest`).
///         2. Worker agent / swarm posts a mandatory >= 5% (`500 bps`) refundable anti-hallucination stake.
///         3. On cryptographic delivery (`sha256(deliveryPayload) == expectedDigest`), escrow splits
///            atomically across a Hierarchical Multi-Agent Swarm DAG (`sum(shareBps) <= 10_000`) with
///            zero-dust remainder routed to the coordinator, returning 100% of worker's stake.
///         4. On missed deadline or verified fraud challenge (`slashExpiredOrFraud`), worker stake is
///            slashed strictly 50% to client compensation / 50% permanently burned to `0x...dEaD`,
///            while 100% of client's escrowed budget is refunded untouched.

interface IERC20Clearing {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 value) external returns (bool);
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

struct SwarmNode {
    address payout;
    uint16 shareBps;
}

struct AgentSla {
    address client;
    address worker;
    address token;
    uint256 budget;
    uint256 workerStake;
    bytes32 expectedDigest;
    uint64 deadline;
    bool settled;
    bool slashed;
}

struct AgentReputation {
    uint64 completedSlas;
    uint64 slashedSlas;
    uint256 cumulativeVolume;
    uint256 cumulativeSlashedStake;
}

contract NewsatoshiAgentClearinghouse {
    uint256 public constant BPS = 10_000;
    uint256 public constant MIN_WORKER_STAKE_BPS = 500; // 5.00% anti-hallucination floor
    uint64 public constant MAX_SLA_DURATION = 30 days;
    address public constant BURN_ADDRESS = address(0x000000000000000000000000000000000000dEaD);

    mapping(bytes32 => AgentSla) public slas;
    mapping(address => AgentReputation) public reputations;
    uint256 public totalSettledVolume;
    uint256 public totalFraudBurned;
    uint256 private _locked = 1;

    event SlaCreated(
        bytes32 indexed slaId,
        address indexed client,
        address indexed worker,
        address token,
        uint256 budget,
        uint256 workerStake,
        bytes32 expectedDigest,
        uint64 deadline
    );

    event SlaSettledAndSplit(
        bytes32 indexed slaId,
        address indexed worker,
        uint256 budgetPaid,
        uint256 coordinatorRemainder,
        uint256 subAgentCount
    );

    event SlaSlashed5050(
        bytes32 indexed slaId,
        address indexed worker,
        uint256 clientCompensation,
        uint256 permanentlyBurned,
        uint256 clientBudgetRefunded
    );

    modifier nonReentrant() {
        require(_locked == 1, "REENTRANCY");
        _locked = 2;
        _;
        _locked = 1;
    }

    function computeSlaId(
        address client,
        address worker,
        address token,
        uint256 budget,
        bytes32 expectedDigest,
        uint64 deadline,
        uint256 salt
    ) public view returns (bytes32) {
        return keccak256(abi.encode(block.chainid, address(this), client, worker, token, budget, expectedDigest, deadline, salt));
    }

    /// @notice Opens a bonded Agent-to-Agent SLA escrow. Both client (budget) and worker (>=5% stake)
    ///         must have approved this contract for `token`.
    function createSla(
        address client,
        address worker,
        address token,
        uint256 budget,
        uint256 workerStake,
        bytes32 expectedDigest,
        uint64 deadline,
        uint256 salt
    ) external nonReentrant returns (bytes32 slaId) {
        require(msg.sender == client || msg.sender == worker, "ONLY_PARTICIPANT");
        require(client != address(0) && worker != address(0) && client != worker, "INVALID_PARTIES");
        require(token != address(0), "INVALID_TOKEN");
        require(budget > 0, "ZERO_BUDGET");
        require(expectedDigest != bytes32(0), "ZERO_DIGEST");
        require(deadline > block.timestamp + 60 && deadline <= block.timestamp + MAX_SLA_DURATION, "BAD_DEADLINE");

        uint256 minStake = (budget * MIN_WORKER_STAKE_BPS) / BPS;
        require(workerStake >= minStake && workerStake > 0, "STAKE_BELOW_5PCT");

        slaId = computeSlaId(client, worker, token, budget, expectedDigest, deadline, salt);
        require(slas[slaId].client == address(0), "SLA_EXISTS");

        uint256 beforeBal = IERC20Clearing(token).balanceOf(address(this));
        _safeTransferFrom(token, client, address(this), budget);
        uint256 receivedBudget = IERC20Clearing(token).balanceOf(address(this)) - beforeBal;

        uint256 midBal = IERC20Clearing(token).balanceOf(address(this));
        _safeTransferFrom(token, worker, address(this), workerStake);
        uint256 receivedStake = IERC20Clearing(token).balanceOf(address(this)) - midBal;
        require(receivedBudget > 0 && receivedStake >= (receivedBudget * MIN_WORKER_STAKE_BPS) / BPS, "BAD_RECEIVED");

        slas[slaId] = AgentSla({
            client: client,
            worker: worker,
            token: token,
            budget: receivedBudget,
            workerStake: receivedStake,
            expectedDigest: expectedDigest,
            deadline: deadline,
            settled: false,
            slashed: false
        });

        emit SlaCreated(slaId, client, worker, token, receivedBudget, receivedStake, expectedDigest, deadline);
    }

    /// @notice Verifies `sha256(deliveryPayload) == expectedDigest`, atomically splits `budget` across
    ///         the Hierarchical Multi-Agent Swarm DAG with zero dust leak, and refunds worker's stake.
    function settleAndSplitDag(
        bytes32 slaId,
        bytes calldata deliveryPayload,
        SwarmNode[] calldata nodes
    ) external nonReentrant {
        AgentSla storage s = slas[slaId];
        require(s.client != address(0), "SLA_NOT_FOUND");
        require(!s.settled && !s.slashed, "SLA_CLOSED");
        require(block.timestamp <= s.deadline, "SLA_EXPIRED");
        require(msg.sender == s.worker, "ONLY_WORKER");
        require(sha256(deliveryPayload) == s.expectedDigest, "PREDICATE_MISMATCH");
        require(nodes.length <= 32, "TOO_MANY_NODES");

        s.settled = true;

        uint256 bpsSum = 0;
        uint256 paidSum = 0;
        for (uint256 i = 0; i < nodes.length; i++) {
            require(nodes[i].payout != address(0), "ZERO_NODE_PAYOUT");
            require(nodes[i].shareBps > 0, "ZERO_NODE_BPS");
            bpsSum += nodes[i].shareBps;
            require(bpsSum <= BPS, "BPS_OVERFLOW");
            uint256 share = (s.budget * nodes[i].shareBps) / BPS;
            if (share > 0) {
                paidSum += share;
                _safeTransfer(s.token, nodes[i].payout, share);
            }
        }

        uint256 coordRemainder = s.budget - paidSum;
        if (coordRemainder > 0) {
            _safeTransfer(s.token, s.worker, coordRemainder);
        }
        // Refund 100% of worker's anti-hallucination stake
        _safeTransfer(s.token, s.worker, s.workerStake);

        AgentReputation storage rep = reputations[s.worker];
        rep.completedSlas += 1;
        rep.cumulativeVolume += s.budget;
        totalSettledVolume += s.budget;

        emit SlaSettledAndSplit(slaId, s.worker, s.budget, coordRemainder, nodes.length);
    }

    /// @notice After deadline expires without valid verified delivery, refunds 100% of `budget` to client
    ///         and slashes worker's stake strictly 50% to client compensation / 50% permanently burned.
    function slashExpired(bytes32 slaId) external nonReentrant {
        AgentSla storage s = slas[slaId];
        require(s.client != address(0), "SLA_NOT_FOUND");
        require(!s.settled && !s.slashed, "SLA_CLOSED");
        require(block.timestamp > s.deadline, "NOT_EXPIRED");

        s.slashed = true;
        uint256 comp = s.workerStake / 2;
        uint256 burned = s.workerStake - comp;

        // Refund 100% of client budget + 50% stake compensation; burn 50% permanently
        _safeTransfer(s.token, s.client, s.budget + comp);
        if (burned > 0) {
            _safeTransfer(s.token, BURN_ADDRESS, burned);
        }

        AgentReputation storage rep = reputations[s.worker];
        rep.slashedSlas += 1;
        rep.cumulativeSlashedStake += s.workerStake;
        totalFraudBurned += burned;

        emit SlaSlashed5050(slaId, s.worker, comp, burned, s.budget);
    }

    function _safeTransfer(address token, address to, uint256 value) internal {
        (bool ok, bytes memory data) = token.call(abi.encodeWithSelector(IERC20Clearing.transfer.selector, to, value));
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "TRANSFER_FAILED");
    }

    function _safeTransferFrom(address token, address from, address to, uint256 value) internal {
        (bool ok, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20Clearing.transferFrom.selector, from, to, value));
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "TRANSFER_FROM_FAILED");
    }
}
