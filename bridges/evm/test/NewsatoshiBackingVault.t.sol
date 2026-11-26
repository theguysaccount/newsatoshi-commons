// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {NewsatoshiBackingVault} from "../NewsatoshiBackingVault.sol";

contract MockVaultUsdc {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        require(balanceOf[from] >= amount, "LOW_BAL");
        require(allowance[from][msg.sender] >= amount, "LOW_ALLOW");
        allowance[from][msg.sender] -= amount;
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

contract NewsatoshiBackingVaultTest {
    NewsatoshiBackingVault internal vault;
    MockVaultUsdc internal usdc;

    function setUp() public {
        vault = new NewsatoshiBackingVault();
        usdc = new MockVaultUsdc();
        usdc.mint(address(this), 1_000_000_000);
    }

    function test_DepositEthAndReceiveCreditGlobalAndCharter() public {
        bytes32 charterId = keccak256("charter-1");
        bytes32 memo = keccak256("genesis-backing");

        vault.depositEth{value: 0.01 ether}(charterId, memo);
        (bool ok,) = address(vault).call{value: 0.005 ether}("");
        require(ok, "raw receive failed");

        (uint256 totalRes, uint256 depCount, uint256 anchoredCount) = vault.getReserveSummary(address(0));
        require(totalRes == 0.015 ether, "global ETH mismatch");
        require(depCount == 2, "deposit count mismatch");
        require(anchoredCount == 0, "anchored count should be 0");
        require(address(vault).balance == 0.015 ether, "native balance mismatch");
        require(vault.charterReserveByToken(charterId, address(0)) == 0.01 ether, "charter ETH mismatch");
        require(vault.charterReserveByToken(bytes32(0), address(0)) == 0.005 ether, "global sentinel mismatch");
    }

    function test_DepositErc20Accounting() public {
        bytes32 charterId = keccak256("charter-usdc");
        usdc.approve(address(vault), 50_000_000);
        uint256 received = vault.depositErc20(charterId, address(usdc), 50_000_000, bytes32(uint256(1)));
        require(received == 50_000_000, "received mismatch");
        require(vault.totalReserveByToken(address(usdc)) == 50_000_000, "global USDC mismatch");
        require(vault.charterReserveByToken(charterId, address(usdc)) == 50_000_000, "charter USDC mismatch");
    }

    function test_RegisterCharterAnchorIsImmutableFirstWriterWins() public {
        bytes32 charterId = keccak256("web4-business-1");
        bytes32 constHash = sha256("constitution-v1");
        bytes32 covHash = sha256("covenant-v2.1");
        string memory uri = "https://commons.newsatoshi.org/api/web4/charters/web4-1/erc8004.json";

        vault.registerCharterAnchor(charterId, constHash, covHash, 2000, uri);
        (
            uint256 res,
            uint256 deps,
            bytes32 gotConst,
            bytes32 gotCov,
            uint16 gotBps,
            address gotCreator
        ) = vault.getCharterSummary(charterId, address(0));
        require(res == 0 && deps == 0, "empty initial reserves");
        require(gotConst == constHash && gotCov == covHash, "hash mismatch");
        require(gotBps == 2000 && gotCreator == address(this), "metadata mismatch");
        require(vault.totalAnchoredCharters() == 1, "anchored count mismatch");

        // Second registration on same charterId MUST revert (immutability!)
        (bool dupOk,) = address(vault).call(
            abi.encodeWithSelector(
                vault.registerCharterAnchor.selector,
                charterId,
                sha256("evil"),
                covHash,
                100,
                uri
            )
        );
        require(!dupOk, "duplicate anchor must revert");
    }

    function test_RejectZeroAmountAndInvalidBps() public {
        (bool zeroEth,) = address(vault).call{value: 0}(
            abi.encodeWithSelector(vault.depositEth.selector, bytes32(0), bytes32(0))
        );
        require(!zeroEth, "zero ETH must revert");

        (bool badBps,) = address(vault).call(
            abi.encodeWithSelector(
                vault.registerCharterAnchor.selector,
                keccak256("bad"),
                sha256("c"),
                sha256("r"),
                uint16(10001),
                "https://commons.newsatoshi.org"
            )
        );
        require(!badBps, "bps > 10000 must revert");
    }

    function testFuzz_MultiCharterDepositConservation(uint96 a, uint96 b) public {
        if (a == 0 || b == 0) return;
        uint256 ua = uint256(a);
        uint256 ub = uint256(b);
        usdc.mint(address(this), ua + ub);
        usdc.approve(address(vault), ua + ub);
        vault.depositErc20(keccak256("cA"), address(usdc), ua, bytes32(0));
        vault.depositErc20(keccak256("cB"), address(usdc), ub, bytes32(0));
        require(vault.totalReserveByToken(address(usdc)) == ua + ub, "fuzz conservation failed");
    }
}
